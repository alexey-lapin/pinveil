import com.github.jengelman.gradle.plugins.shadow.tasks.ShadowJar
import org.graalvm.buildtools.gradle.tasks.BuildNativeImageTask

plugins {
    alias(libs.plugins.micronaut.application)
    alias(libs.plugins.micronaut.aot)
    alias(libs.plugins.shadow)
    alias(libs.plugins.release)
    id("gg.jte.gradle") version "3.2.3"
}

version = scmVersion.version

dependencies {
    annotationProcessor("io.micronaut:micronaut-http-validation")
    annotationProcessor("io.micronaut.serde:micronaut-serde-processor")

    compileOnly("io.micronaut:micronaut-http-client")

    implementation("de.mkammerer:argon2-jvm:2.12")
    implementation("io.micronaut:micronaut-http-validation")
    implementation("io.micronaut:micronaut-runtime")
    implementation("io.micronaut.serde:micronaut-serde-jackson")
    implementation("io.micronaut.views:micronaut-views-jte")
    implementation("io.micronaut.views:micronaut-views-htmx")

    runtimeOnly("ch.qos.logback:logback-classic")

    testImplementation("io.micronaut:micronaut-http-client")
    testImplementation("io.micronaut.test:micronaut-test-junit5")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

application {
    mainClass = "com.example.Application"
}

java {
    sourceCompatibility = JavaVersion.toVersion("25")
    targetCompatibility = JavaVersion.toVersion("25")
}

graalvmNative {
    toolchainDetection.set(true)
    binaries {
        named("main") {
            imageName.set(rootProject.name)
            buildArgs.add("--verbose")
        }
    }
}

micronaut {
    runtime("netty")
    testRuntime("junit5")
    processing {
        incremental(true)
        annotations("com.example.*")
    }
    aot {
        // Please review carefully the optimizations enabled below
        // Check https://micronaut-projects.github.io/micronaut-aot/latest/guide/ for more details
        optimizeServiceLoading = false
        convertYamlToJava = false
        precomputeOperations = true
        cacheEnvironment = true
        optimizeClassLoading = true
        deduceEnvironment = true
        optimizeNetty = true
        replaceLogbackXml = true
    }
}

tasks {

    named<ShadowJar>("shadowJar") {
        archiveBaseName = rootProject.name
        archiveClassifier = "dist"
    }

    val writeArtifactFile by registering {
        doLast {
            val outputDirectory = getByName<BuildNativeImageTask>("nativeCompile").outputDirectory
            outputDirectory.get().asFile.mkdirs()
            outputDirectory.file("gradle-artifact.txt")
                .get().asFile
                .writeText("${rootProject.name}-${project.version}-${platform()}")
        }
    }

    named("nativeCompile") {
        finalizedBy(writeArtifactFile)
    }

}

jte {
    sourceDirectory = file("src/main/jte").toPath()
    generate()
}

// Gradle requires that generateJte is run before some tasks
tasks.configureEach {
    if (name == "inspectRuntimeClasspath") {
        mustRunAfter("generateJte")
    }
}

fun platform(): String {
    val os = org.gradle.internal.os.OperatingSystem.current()
    val arc = System.getProperty("os.arch")
    return when {
        org.gradle.internal.os.OperatingSystem.current().isWindows -> "windows-${arc}"
        org.gradle.internal.os.OperatingSystem.current().isLinux -> "linux-${arc}"
        org.gradle.internal.os.OperatingSystem.current().isMacOsX -> "darwin-${arc}"
        else -> os.nativePrefix
    }
}
