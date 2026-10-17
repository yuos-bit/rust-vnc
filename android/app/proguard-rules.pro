# Keep protocol DTOs (serialized with kotlinx.serialization) when minification
# is enabled in a future release build.
-keepattributes *Annotation*, InnerClasses
-dontnote kotlinx.serialization.**
-keep,includedescriptorclasses class com.rustvnc.client.**$$serializer { *; }
-keepclassmembers class com.rustvnc.client.** {
    *** Companion;
}
-keepclasseswithmembers class com.rustvnc.client.** {
    kotlinx.serialization.KSerializer serializer(...);
}
