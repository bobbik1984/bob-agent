plugins {
    id("com.android.library")
    kotlin("android")
}
android {
    namespace = "org.bobbik.devicevault"
    compileSdk = 35
    defaultConfig { minSdk = 24 }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}
dependencies { compileOnly(project(":tauri-android")) }
