package com.rustvnc.client.ui.nav

import androidx.compose.runtime.Composable
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument
import com.rustvnc.client.AppContainer
import com.rustvnc.client.ui.devices.DeviceListRoute
import com.rustvnc.client.ui.pairing.PairingRoute
import com.rustvnc.client.ui.session.SessionRoute
import com.rustvnc.client.ui.settings.SettingsRoute

@Composable
fun AppNavHost(container: AppContainer) {
    val navController = rememberNavController()

    NavHost(
        navController = navController,
        startDestination = Routes.DEVICES,
    ) {
        composable(Routes.DEVICES) {
            DeviceListRoute(
                repository = container.repository,
                tailscaleIntegrated = container.tailscale.isIntegrated,
                onAddDevice = { navController.navigate(Routes.PAIRING) { launchSingleTop = true } },
                onOpenDevice = { code ->
                    navController.navigate("${Routes.SESSION_PREFIX}$code") { launchSingleTop = true }
                },
                onSettings = { navController.navigate(Routes.SETTINGS) { launchSingleTop = true } },
            )
        }

        composable(Routes.PAIRING) {
            PairingRoute(
                repository = container.repository,
                onBack = { navController.popBackStack() },
                onSettings = { navController.navigate(Routes.SETTINGS) { launchSingleTop = true } },
                onPaired = { navController.popBackStack() },
            )
        }

        composable(Routes.SETTINGS) {
            SettingsRoute(
                settings = container.settings,
                secure = container.secure,
                tailscale = container.tailscale,
                api = container.api,
                onBack = { navController.popBackStack() },
            )
        }

        composable(
            route = Routes.SESSION,
            arguments = listOf(navArgument("code") { type = NavType.StringType }),
        ) { entry ->
            val code = entry.arguments?.getString("code").orEmpty()
            SessionRoute(
                repository = container.repository,
                tailscale = container.tailscale,
                deviceCode = code,
                onBack = { navController.popBackStack() },
                onSettings = { navController.navigate(Routes.SETTINGS) { launchSingleTop = true } },
            )
        }
    }
}

private object Routes {
    const val DEVICES = "devices"
    const val PAIRING = "pairing"
    const val SETTINGS = "settings"
    const val SESSION = "session/{code}"
    const val SESSION_PREFIX = "session/"
}
