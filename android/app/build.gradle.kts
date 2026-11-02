import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "app.bookworm"
    compileSdk = 36

    defaultConfig {
        applicationId = "app.bookworm"
        // Canvas.drawGlyphs (API 31) понадобится, когда отрисовка глифов переедет в Android.
        minSdk = 31
        targetSdk = 36
        versionCode = 11
        versionName = "0.5.4"
        // Телефон — arm64; для эмулятора: -Pabis=arm64-v8a,x86_64
        ndk { abiFilters += (findProperty("abis") as String? ?: "arm64-v8a").split(",") }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            // Личное приложение: подписываем отладочным ключом, магазина нет.
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    sourceSets["test"].resources.srcDir("../../tests/fixtures")

    // Шрифты и книги ядро читает с диска; не сжимаем, чтобы копировать быстро.
    androidResources { noCompress += listOf("ttf", "fb2", "zip") }
}

kotlin {
    compilerOptions { jvmTarget.set(JvmTarget.JVM_17) }
}

dependencies {
    testImplementation("junit:junit:4.13.2")
}
