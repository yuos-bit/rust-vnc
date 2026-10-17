// Command rvnc-tsnet is the RustVNC Tailscale sidecar for desktop endpoints.
//
// It joins the tailnet through tailscale.com/tsnet and provides three local
// services:
//
//   - a tailnet listener (default :21416) that forwards inbound controller
//     connections to the Rust agent on loopback (default 127.0.0.1:21416);
//   - a loopback SOCKS5 CONNECT proxy (default 127.0.0.1:1055) that the Rust
//     controller uses to dial 100.x.y.z addresses inside the tailnet;
//   - a loopback HTTP status endpoint (default 127.0.0.1:1056) exposing
//     /healthz and /whoami for the desktop application.
//
// The process never logs TS_AUTHKEY or any pairing secret.
package main

import (
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net"
	"net/http"
	"os"
	"os/signal"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"tailscale.com/tsnet"
)

const (
	defaultHostname   = "rvnc-sidecar"
	defaultStateDir   = "./tsnet-state"
	defaultAgentAddr  = "127.0.0.1:21416"
	defaultListenAddr = ":21416"
	defaultSocksAddr  = "127.0.0.1:1055"
	defaultStatusAddr = "127.0.0.1:1056"

	socksVersion    = 0x05
	socksNoAuth     = 0x00
	socksCmdConnect = 0x01
	socksAtypIPv4   = 0x01
	socksAtypDomain = 0x03
	socksAtypIPv6   = 0x04

	handshakeTimeout = 15 * time.Second
	dialTimeout      = 30 * time.Second
)

type config struct {
	authKey    string
	hostname   string
	stateDir   string
	controlURL string
	derpURL    string
	agentAddr  string
	listenAddr string
	socksAddr  string
	statusAddr string
	logLevel   slog.Level
}

type app struct {
	log   *slog.Logger
	cfg   config
	srv   *tsnet.Server
	conns *connSet
}

func main() {
	if err := run(); err != nil {
		slog.Error("sidecar failed", "err", err)
		os.Exit(1)
	}
}

func run() error {
	cfg, err := loadConfig()
	if err != nil {
		return err
	}

	logger := slog.New(slog.NewTextHandler(os.Stderr, &slog.HandlerOptions{Level: cfg.logLevel}))
	slog.SetDefault(logger)

	if cfg.controlURL == "" {
		logger.Warn("TS_CONTROLURL is empty; tsnet will use the public Tailscale control plane. Set it to your Headscale URL in production")
	}
	if cfg.derpURL != "" {
		logger.Warn("TS_DERP_URL is accepted for compatibility but tsnet does not apply it directly; add the DERP region to Headscale's derp.urls instead",
			"derp_url", cfg.derpURL)
	}
	if !isLoopbackListenAddr(cfg.socksAddr) {
		logger.Warn("SOCKS5 listener is not bound to loopback; it has no authentication",
			"socks_addr", cfg.socksAddr)
	}

	if err := os.MkdirAll(cfg.stateDir, 0o700); err != nil {
		return fmt.Errorf("create state dir %q: %w", cfg.stateDir, err)
	}

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	srv := &tsnet.Server{
		Dir:        cfg.stateDir,
		Hostname:   cfg.hostname,
		AuthKey:    cfg.authKey,
		ControlURL: cfg.controlURL,
		Logf: func(format string, args ...any) {
			logger.Debug("tsnet", "msg", fmt.Sprintf(format, args...))
		},
	}
	defer srv.Close()

	logger.Info("joining tailnet",
		"hostname", cfg.hostname,
		"state_dir", cfg.stateDir,
		"control_url", cfg.controlURL,
	)
	status, err := srv.Up(ctx)
	if err != nil {
		return fmt.Errorf("join tailnet: %w", err)
	}

	ownIP4 := ""
	backendState := ""
	if status != nil {
		backendState = status.BackendState
		if status.Self != nil {
			for _, ip := range status.Self.TailscaleIPs {
				if ip.Is4() {
					ownIP4 = ip.String()
					break
				}
			}
		}
	}
	logger.Info("tailnet is up", "ip4", ownIP4, "backend_state", backendState)

	// Controllers reach the Rust agent through this tailnet listener.
	tailLn, err := srv.Listen("tcp", cfg.listenAddr)
	if err != nil {
		return fmt.Errorf("listen on tailnet %q: %w", cfg.listenAddr, err)
	}

	// The Rust controller dials the tailnet through this loopback proxy.
	socksLn, err := net.Listen("tcp", cfg.socksAddr)
	if err != nil {
		_ = tailLn.Close()
		return fmt.Errorf("listen for SOCKS5 on %q: %w", cfg.socksAddr, err)
	}

	statusLn, err := net.Listen("tcp", cfg.statusAddr)
	if err != nil {
		_ = tailLn.Close()
		_ = socksLn.Close()
		return fmt.Errorf("listen for status on %q: %w", cfg.statusAddr, err)
	}

	a := &app{
		log:   logger,
		cfg:   cfg,
		srv:   srv,
		conns: newConnSet(),
	}

	mux := http.NewServeMux()
	mux.HandleFunc("/healthz", a.handleHealth)
	mux.HandleFunc("/whoami", a.handleWhoami)

	httpServer := &http.Server{
		Handler:           mux,
		ReadHeaderTimeout: 5 * time.Second,
		ReadTimeout:       10 * time.Second,
		WriteTimeout:      10 * time.Second,
		IdleTimeout:       30 * time.Second,
	}

	logger.Info("sidecar ready",
		"tailnet_listen", tailLn.Addr().String(),
		"agent", cfg.agentAddr,
		"socks5", socksLn.Addr().String(),
		"status", statusLn.Addr().String(),
	)

	runCtx, cancel := context.WithCancel(ctx)
	defer cancel()

	errCh := make(chan error, 3)
	go func() { errCh <- a.forwardLoop(runCtx, tailLn) }()
	go func() { errCh <- a.serveSOCKS(runCtx, socksLn) }()
	go func() {
		err := httpServer.Serve(statusLn)
		if errors.Is(err, http.ErrServerClosed) {
			err = nil
		}
		errCh <- err
	}()

	var serveErr error
	select {
	case <-ctx.Done():
		logger.Info("shutdown signal received")
	case serveErr = <-errCh:
		if serveErr != nil {
			logger.Error("sidecar listener stopped", "err", serveErr)
		}
	}

	// Stop accepting, then tear down in-flight connections and the tailnet.
	cancel()
	shutdownCtx, shutdownCancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer shutdownCancel()
	_ = httpServer.Shutdown(shutdownCtx)
	_ = tailLn.Close()
	_ = socksLn.Close()
	a.conns.closeAll()

	logger.Info("sidecar stopped")
	return serveErr
}

func loadConfig() (config, error) {
	cfg := config{
		authKey:    strings.TrimSpace(os.Getenv("TS_AUTHKEY")),
		hostname:   envOrDefault("TS_HOSTNAME", defaultHostname),
		stateDir:   envOrDefault("TS_STATE_DIR", defaultStateDir),
		controlURL: strings.TrimRight(strings.TrimSpace(os.Getenv("TS_CONTROLURL")), "/"),
		derpURL:    strings.TrimSpace(os.Getenv("TS_DERP_URL")),
		agentAddr:  envOrDefault("RVNC_AGENT_ADDR", defaultAgentAddr),
		listenAddr: envOrDefault("RVNC_LISTEN_ADDR", defaultListenAddr),
		socksAddr:  envOrDefault("RVNC_SOCKS_ADDR", defaultSocksAddr),
		statusAddr: envOrDefault("RVNC_STATUS_ADDR", defaultStatusAddr),
		logLevel:   parseLevel(envOrDefault("RVNC_LOG_LEVEL", "info")),
	}

	if cfg.authKey == "" {
		return cfg, errors.New("TS_AUTHKEY is required (use the pre-auth key returned by /api/v1/devices/register)")
	}
	if strings.TrimSpace(cfg.hostname) == "" {
		return cfg, errors.New("TS_HOSTNAME must not be empty")
	}

	addresses := []struct {
		name string
		addr string
	}{
		{"RVNC_AGENT_ADDR", cfg.agentAddr},
		{"RVNC_LISTEN_ADDR", cfg.listenAddr},
		{"RVNC_SOCKS_ADDR", cfg.socksAddr},
		{"RVNC_STATUS_ADDR", cfg.statusAddr},
	}
	for _, item := range addresses {
		if _, _, err := net.SplitHostPort(item.addr); err != nil {
			return cfg, fmt.Errorf("%s=%q is not a host:port address: %w", item.name, item.addr, err)
		}
	}

	// The status endpoint exposes WhoIs data, so it must stay loopback-only.
	if !isLoopbackListenAddr(cfg.statusAddr) {
		return cfg, fmt.Errorf("RVNC_STATUS_ADDR %q must bind a loopback address", cfg.statusAddr)
	}

	return cfg, nil
}

func envOrDefault(key, fallback string) string {
	value := strings.TrimSpace(os.Getenv(key))
	if value == "" {
		return fallback
	}
	return value
}

func parseLevel(value string) slog.Level {
	switch strings.ToLower(strings.TrimSpace(value)) {
	case "debug":
		return slog.LevelDebug
	case "warn", "warning":
		return slog.LevelWarn
	case "error":
		return slog.LevelError
	default:
		return slog.LevelInfo
	}
}

func isLoopbackListenAddr(addr string) bool {
	host, _, err := net.SplitHostPort(addr)
	if err != nil {
		return false
	}
	if strings.EqualFold(host, "localhost") {
		return true
	}
	ip := net.ParseIP(host)
	return ip != nil && ip.IsLoopback()
}

// ---------------------------------------------------------------------------
// Tailnet -> local agent forwarding
// ---------------------------------------------------------------------------

func (a *app) forwardLoop(ctx context.Context, ln net.Listener) error {
	for {
		conn, err := ln.Accept()
		if err != nil {
			if ctx.Err() != nil {
				return nil
			}
			return fmt.Errorf("tailnet accept: %w", err)
		}
		go a.forwardConn(ctx, conn)
	}
}

func (a *app) forwardConn(ctx context.Context, inbound net.Conn) {
	if !a.conns.add(inbound) {
		_ = inbound.Close()
		return
	}
	defer func() {
		a.conns.remove(inbound)
		_ = inbound.Close()
	}()

	a.log.Debug("tailnet connection accepted", "remote", inbound.RemoteAddr().String())

	dialCtx, cancel := context.WithTimeout(ctx, dialTimeout)
	defer cancel()

	dialer := net.Dialer{Timeout: dialTimeout, KeepAlive: 30 * time.Second}
	outbound, err := dialer.DialContext(dialCtx, "tcp", a.cfg.agentAddr)
	if err != nil {
		a.log.Warn("cannot reach the local Rust agent",
			"agent", a.cfg.agentAddr,
			"remote", inbound.RemoteAddr().String(),
			"err", err,
		)
		return
	}
	if !a.conns.add(outbound) {
		_ = outbound.Close()
		return
	}
	defer func() {
		a.conns.remove(outbound)
		_ = outbound.Close()
	}()

	pipeBoth(inbound, outbound)
}

// ---------------------------------------------------------------------------
// SOCKS5 CONNECT server (RFC 1928, no authentication)
// ---------------------------------------------------------------------------

func (a *app) serveSOCKS(ctx context.Context, ln net.Listener) error {
	for {
		conn, err := ln.Accept()
		if err != nil {
			if ctx.Err() != nil {
				return nil
			}
			return fmt.Errorf("socks5 accept: %w", err)
		}
		go a.handleSOCKSConn(ctx, conn)
	}
}

func (a *app) handleSOCKSConn(ctx context.Context, conn net.Conn) {
	if !a.conns.add(conn) {
		_ = conn.Close()
		return
	}
	defer func() {
		a.conns.remove(conn)
		_ = conn.Close()
	}()

	_ = conn.SetDeadline(time.Now().Add(handshakeTimeout))
	host, port, err := socksHandshake(conn)
	if err != nil {
		a.log.Debug("socks5 handshake failed",
			"remote", conn.RemoteAddr().String(),
			"err", err,
		)
		return
	}
	_ = conn.SetDeadline(time.Time{})

	dest := net.JoinHostPort(host, strconv.Itoa(int(port)))
	dialCtx, cancel := context.WithTimeout(ctx, dialTimeout)
	defer cancel()

	// srv.Dial resolves both IPs and MagicDNS names inside the tailnet.
	outbound, err := a.srv.Dial(dialCtx, "tcp", dest)
	if err != nil {
		code := socksReplyCode(err)
		a.log.Debug("socks5 connect failed", "dest", dest, "err", err)
		_ = writeSOCKSReply(conn, code, nil)
		return
	}
	if !a.conns.add(outbound) {
		_ = outbound.Close()
		return
	}
	defer func() {
		a.conns.remove(outbound)
		_ = outbound.Close()
	}()

	if err := writeSOCKSReply(conn, 0x00, outbound.LocalAddr()); err != nil {
		a.log.Debug("socks5 reply failed", "dest", dest, "err", err)
		return
	}

	a.log.Debug("socks5 connected", "dest", dest)
	pipeBoth(conn, outbound)
}

// socksHandshake performs method negotiation and parses one CONNECT request.
// It returns the destination host (IP literal or domain name) and port.
func socksHandshake(conn net.Conn) (string, uint16, error) {
	var header [2]byte
	if _, err := io.ReadFull(conn, header[:]); err != nil {
		return "", 0, err
	}
	if header[0] != socksVersion {
		return "", 0, fmt.Errorf("unsupported SOCKS version %d", header[0])
	}
	methodCount := int(header[1])
	if methodCount == 0 {
		return "", 0, errors.New("client offered no authentication methods")
	}
	methods := make([]byte, methodCount)
	if _, err := io.ReadFull(conn, methods); err != nil {
		return "", 0, err
	}

	offersNoAuth := false
	for _, method := range methods {
		if method == socksNoAuth {
			offersNoAuth = true
			break
		}
	}
	if !offersNoAuth {
		_, _ = conn.Write([]byte{socksVersion, 0xFF})
		return "", 0, errors.New("no acceptable authentication method")
	}
	if _, err := conn.Write([]byte{socksVersion, socksNoAuth}); err != nil {
		return "", 0, err
	}

	var request [4]byte
	if _, err := io.ReadFull(conn, request[:]); err != nil {
		return "", 0, err
	}
	if request[0] != socksVersion {
		return "", 0, fmt.Errorf("bad request version %d", request[0])
	}
	if request[1] != socksCmdConnect {
		_ = writeSOCKSReply(conn, 0x07, nil) // command not supported
		return "", 0, fmt.Errorf("unsupported command %d", request[1])
	}

	var host string
	switch request[3] {
	case socksAtypIPv4:
		addr := make([]byte, net.IPv4len)
		if _, err := io.ReadFull(conn, addr); err != nil {
			return "", 0, err
		}
		host = net.IP(addr).String()
	case socksAtypIPv6:
		addr := make([]byte, net.IPv6len)
		if _, err := io.ReadFull(conn, addr); err != nil {
			return "", 0, err
		}
		host = net.IP(addr).String()
	case socksAtypDomain:
		var length [1]byte
		if _, err := io.ReadFull(conn, length[:]); err != nil {
			return "", 0, err
		}
		if length[0] == 0 {
			_ = writeSOCKSReply(conn, 0x08, nil) // address type not supported
			return "", 0, errors.New("empty domain name")
		}
		name := make([]byte, int(length[0]))
		if _, err := io.ReadFull(conn, name); err != nil {
			return "", 0, err
		}
		host = string(name)
	default:
		_ = writeSOCKSReply(conn, 0x08, nil)
		return "", 0, fmt.Errorf("unsupported address type %d", request[3])
	}

	var portBytes [2]byte
	if _, err := io.ReadFull(conn, portBytes[:]); err != nil {
		return "", 0, err
	}
	port := binary.BigEndian.Uint16(portBytes[:])
	if port == 0 {
		_ = writeSOCKSReply(conn, 0x01, nil) // general failure
		return "", 0, errors.New("port 0 is not a valid destination")
	}

	return host, port, nil
}

// writeSOCKSReply sends a CONNECT reply. A nil addr produces 0.0.0.0:0.
func writeSOCKSReply(w io.Writer, reply byte, addr net.Addr) error {
	var ip net.IP
	var port int
	if tcpAddr, ok := addr.(*net.TCPAddr); ok && tcpAddr != nil {
		ip = tcpAddr.IP
		port = tcpAddr.Port
	}

	if ip4 := ip.To4(); ip4 != nil {
		buf := make([]byte, 10)
		buf[0] = socksVersion
		buf[1] = reply
		buf[2] = 0x00
		buf[3] = socksAtypIPv4
		copy(buf[4:8], ip4)
		binary.BigEndian.PutUint16(buf[8:10], uint16(port))
		_, err := w.Write(buf)
		return err
	}
	if ip16 := ip.To16(); ip16 != nil {
		buf := make([]byte, 22)
		buf[0] = socksVersion
		buf[1] = reply
		buf[2] = 0x00
		buf[3] = socksAtypIPv6
		copy(buf[4:20], ip16)
		binary.BigEndian.PutUint16(buf[20:22], uint16(port))
		_, err := w.Write(buf)
		return err
	}

	// Unknown bound address: reply with 0.0.0.0:0, as permitted in practice.
	_, err := w.Write([]byte{socksVersion, reply, 0x00, socksAtypIPv4, 0, 0, 0, 0, 0, 0})
	return err
}

func socksReplyCode(err error) byte {
	switch {
	case errors.Is(err, syscall.ECONNREFUSED):
		return 0x05 // connection refused
	case errors.Is(err, syscall.ENETUNREACH):
		return 0x03 // network unreachable
	case errors.Is(err, syscall.EHOSTUNREACH):
		return 0x04 // host unreachable
	case errors.Is(err, context.DeadlineExceeded), errors.Is(err, os.ErrDeadlineExceeded):
		return 0x06 // TTL expired / timeout
	}

	// Windows sockets report Winsock codes. Go's POSIX-named constants on
	// Windows are synthetic and do not compare equal to these native values.
	if runtime.GOOS == "windows" {
		var errno syscall.Errno
		if errors.As(err, &errno) {
			switch uint32(errno) {
			case 10061: // WSAECONNREFUSED
				return 0x05
			case 10051: // WSAENETUNREACH
				return 0x03
			case 10065: // WSAEHOSTUNREACH
				return 0x04
			case 10060: // WSAETIMEDOUT
				return 0x06
			}
		}
	}

	var dnsErr *net.DNSError
	if errors.As(err, &dnsErr) {
		return 0x04 // host unreachable
	}
	return 0x01 // general SOCKS server failure
}

// ---------------------------------------------------------------------------
// Loopback HTTP status endpoint
// ---------------------------------------------------------------------------

type healthResponse struct {
	OK       bool   `json:"ok"`
	Hostname string `json:"hostname"`
	IP4      string `json:"ip4"`
	Online   bool   `json:"online"`
}

func (a *app) handleHealth(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		w.Header().Set("Allow", http.MethodGet)
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	ok := false
	online := false
	ip4 := ""

	if local, err := a.srv.LocalClient(); err == nil {
		if status, err := local.Status(r.Context()); err == nil {
			ok = true
			online = status.BackendState == "Running"
			if status.Self != nil {
				online = online && status.Self.Online
				for _, ip := range status.Self.TailscaleIPs {
					if ip.Is4() {
						ip4 = ip.String()
						break
					}
				}
				if ip4 == "" {
					for _, ip := range status.TailscaleIPs {
						if ip.Is4() {
							ip4 = ip.String()
							break
						}
					}
				}
			}
		}
	}

	writeJSON(w, http.StatusOK, healthResponse{
		OK:       ok,
		Hostname: a.cfg.hostname,
		IP4:      ip4,
		Online:   online,
	})
}

type whoamiResponse struct {
	OK    bool   `json:"ok"`
	Self  any    `json:"self,omitempty"`
	Whois any    `json:"whois,omitempty"`
	Error string `json:"error,omitempty"`
}

func (a *app) handleWhoami(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		w.Header().Set("Allow", http.MethodGet)
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	local, err := a.srv.LocalClient()
	if err != nil {
		writeJSON(w, http.StatusInternalServerError, whoamiResponse{OK: false, Error: err.Error()})
		return
	}

	response := whoamiResponse{}
	var statusErr error
	if status, err := local.Status(r.Context()); err != nil {
		statusErr = err
	} else if status.Self != nil {
		response.Self = status.Self
	}

	if info, err := local.WhoIs(r.Context(), r.RemoteAddr); err == nil {
		response.Whois = info
	} else if statusErr != nil {
		writeJSON(w, http.StatusInternalServerError, whoamiResponse{OK: false, Error: err.Error()})
		return
	}

	response.OK = response.Self != nil || response.Whois != nil
	writeJSON(w, http.StatusOK, response)
}

func writeJSON(w http.ResponseWriter, status int, value any) {
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.Header().Set("Cache-Control", "no-store")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(value)
}

// ---------------------------------------------------------------------------
// Connection helpers
// ---------------------------------------------------------------------------

type closeWriter interface {
	CloseWrite() error
}

// pipeBoth copies in both directions and propagates half-close so protocols
// that rely on FIN (for example HTTP/1.0 or a request/response shutdown) work.
func pipeBoth(a, b net.Conn) {
	var wg sync.WaitGroup
	wg.Add(2)
	go func() {
		defer wg.Done()
		copyStream(b, a)
	}()
	go func() {
		defer wg.Done()
		copyStream(a, b)
	}()
	wg.Wait()
}

func copyStream(dst, src net.Conn) {
	_, _ = io.Copy(dst, src)
	if halfCloser, ok := dst.(closeWriter); ok {
		_ = halfCloser.CloseWrite()
		return
	}
	_ = dst.Close()
}

// connSet tracks live connections so shutdown can interrupt in-flight copies.
type connSet struct {
	mu     sync.Mutex
	conns  map[net.Conn]struct{}
	closed bool
}

func newConnSet() *connSet {
	return &connSet{conns: make(map[net.Conn]struct{})}
}

// add returns false when the set has already been closed.
func (s *connSet) add(conn net.Conn) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return false
	}
	s.conns[conn] = struct{}{}
	return true
}

func (s *connSet) remove(conn net.Conn) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.conns, conn)
}

func (s *connSet) closeAll() {
	s.mu.Lock()
	if s.closed {
		s.mu.Unlock()
		return
	}
	s.closed = true
	conns := make([]net.Conn, 0, len(s.conns))
	for conn := range s.conns {
		conns = append(conns, conn)
	}
	s.conns = nil
	s.mu.Unlock()

	for _, conn := range conns {
		_ = conn.Close()
	}
}
