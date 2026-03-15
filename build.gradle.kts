plugins {
    id("com.gradleup.shadow") version "8.3.9"
    id("io.micronaut.application") version "4.6.2"
    id("io.micronaut.aot") version "4.6.2"
    id("gg.jte.gradle") version "3.2.1"
}

version = "0.1"
group = "com.example"

dependencies {
    annotationProcessor("io.micronaut:micronaut-http-validation")
    annotationProcessor("io.micronaut.serde:micronaut-serde-processor")

    compileOnly("io.micronaut:micronaut-http-client")

    implementation("io.micronaut.serde:micronaut-serde-jackson")
    implementation("io.micronaut.views:micronaut-views-jte")
    implementation("io.micronaut.views:micronaut-views-htmx")

    runtimeOnly("ch.qos.logback:logback-classic")

    testImplementation("io.micronaut:micronaut-http-client")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}


application {
    mainClass = "com.example.Application"
}

java {
    sourceCompatibility = JavaVersion.toVersion("25")
    targetCompatibility = JavaVersion.toVersion("25")
}


graalvmNative.toolchainDetection = true

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


tasks.named<io.micronaut.gradle.docker.NativeImageDockerfile>("dockerfileNative") {
    jdkVersion = "25"
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