import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}
val oauthRedirectHost = providers.gradleProperty("oauthRedirectHost").orElse("oauth.invalid").get()
val oauthRedirectPath = providers.gradleProperty("oauthRedirectPath").orElse("/oauth/callback").get()
check(oauthRedirectHost.matches(Regex("[A-Za-z0-9.-]+"))) { "oauthRedirectHost must be a DNS host" }
check(oauthRedirectPath.startsWith("/") && !oauthRedirectPath.contains("#")) {
    "oauthRedirectPath must be an absolute path without a fragment"
}

val syncFrontendAssets by tasks.registering(Sync::class) {
    from(file("../../../../dist"))
    from(file("../../../tauri.conf.json"))
    into(layout.projectDirectory.dir("src/main/assets"))
}

// Every packaging and reporting task reads the synced assets directory,
// so order the sync before the whole build lifecycle instead of naming
// individual consumers (asset merge and lint-model tasks read these outputs).
tasks.named("preBuild").configure {
    dependsOn(syncFrontendAssets)
}

android {
    sourceSets {
        getByName("main").java.srcDir("../../../../android-src")
        getByName("test").java.srcDir("../../../../android-tests")
    }
    signingConfigs {
        create("localTest") {
            storeFile = file(System.getenv("SMS_KEYSTORE") ?: error("SMS_KEYSTORE must point to the local development key"))
            storePassword = "android"
            keyAlias = "androiddebugkey"
            keyPassword = "android"
        }
    }
    compileSdk = 36
    namespace = "dev.local.assistant"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        manifestPlaceholders["oauthRedirectHost"] = oauthRedirectHost
        manifestPlaceholders["oauthRedirectPath"] = oauthRedirectPath
        applicationId = "dev.local.assistant"
        minSdk = 31
        targetSdk = 36
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            check(oauthRedirectHost != "oauth.invalid") {
                "Release builds require -PoauthRedirectHost=<verified App Link host>"
            }
            signingConfig = signingConfigs.getByName("localTest")
            isMinifyEnabled = true
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
        }
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    buildFeatures {
        buildConfig = true
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.browser:browser:1.8.0")
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")
