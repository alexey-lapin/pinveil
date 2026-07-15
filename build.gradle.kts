plugins {
    alias(libs.plugins.release)
}

version = scmVersion.version

subprojects {
    version = rootProject.version
}
