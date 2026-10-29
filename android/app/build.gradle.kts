import org.gradle.api.DefaultTask
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.tasks.OutputDirectory
import org.gradle.api.tasks.TaskAction
import org.gradle.process.ExecOperations
import javax.inject.Inject

plugins {
    id("com.android.application")
}

/** The app shares the desktop version: `version` in the root Cargo.toml. */
val appVersion: String = Regex("""(?m)^version\s*=\s*"([^"]+)"""")
    .find(rootProject.file("../Cargo.toml").readText())!!
    .groupValues[1]
/** `1.2.3` → 10203, so every release installs over the previous one. */
val appVersionCode: Int = appVersion.split("-")[0].split(".")
    .fold(0) { code, part -> code * 100 + part.toInt() }

/**
 * Release signing, from the environment (see .github/workflows/release.yml).
 * Without it the release APK is unsigned. The key must never change: Android
 * only installs updates signed with the same key.
 */
val releaseKeystore: String? = System.getenv("ANDROID_KEYSTORE")

/** ABIs the Rust library is built for (cargo-ndk names). */
val rustAbis = listOf("arm64-v8a", "armeabi-v7a", "x86_64")

android {
    namespace = "com.autofxembed"
    compileSdk = 36
    ndkVersion = "30.0.16248370"

    defaultConfig {
        applicationId = "com.autofxembed"
        minSdk = 24
        targetSdk = 36
        versionCode = appVersionCode
        versionName = appVersion
        ndk { abiFilters += rustAbis }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    signingConfigs {
        if (releaseKeystore != null) {
            create("release") {
                storeFile = file(releaseKeystore)
                storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("ANDROID_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.findByName("release")
        }
    }
}

/**
 * Builds `android/rust` (the JNI bridge to `autofxembed-core`) with cargo-ndk
 * into a jniLibs directory. Needs `cargo-ndk` and the Android Rust targets.
 */
abstract class CargoNdkTask @Inject constructor(private val exec: ExecOperations) : DefaultTask() {
    @get:OutputDirectory
    abstract val outputDir: DirectoryProperty

    @get:org.gradle.api.tasks.Internal
    abstract val workspaceDir: DirectoryProperty

    @get:org.gradle.api.tasks.Internal
    abstract val ndkDir: DirectoryProperty

    @get:org.gradle.api.tasks.Input
    abstract val abis: org.gradle.api.provider.ListProperty<String>

    @TaskAction
    fun build() {
        exec.exec {
            workingDir = workspaceDir.get().asFile
            environment("ANDROID_NDK_HOME", ndkDir.get().asFile.absolutePath)
            val args = mutableListOf("cargo", "ndk", "--platform", "24")
            abis.get().forEach { args += listOf("-t", it) }
            args += listOf(
                "-o", outputDir.get().asFile.absolutePath,
                "build", "--release", "-p", "autofxembed-android",
            )
            commandLine(args)
        }
    }
}

val cargoNdk = tasks.register<CargoNdkTask>("cargoNdk") {
    workspaceDir.set(rootProject.layout.projectDirectory.dir(".."))
    ndkDir.set(androidComponents.sdkComponents.ndkDirectory)
    abis.set(rustAbis)
    // Rust sources live outside Gradle's view: always defer to cargo, whose
    // own incremental build makes an unchanged rebuild quick.
    outputs.upToDateWhen { false }
}

androidComponents {
    onVariants { variant ->
        variant.sources.jniLibs?.addGeneratedSourceDirectory(cargoNdk, CargoNdkTask::outputDir)
    }
}
