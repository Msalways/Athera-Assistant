import org.apache.tools.ant.taskdefs.condition.Os
import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.TaskAction

open class BuildTask : DefaultTask() {
    @Input
    var rootDirRel: String? = null
    @Input
    var target: String? = null
    @Input
    var release: Boolean? = null

    @TaskAction
    fun assemble() {
        val executable = """node""";
        try {
            runTauriCli(executable)
        } catch (e: Exception) {
            if (Os.isFamily(Os.FAMILY_WINDOWS)) {
                // Try different Windows-specific extensions
                val fallbacks = listOf(
                    "$executable.exe",
                    "$executable.cmd",
                    "$executable.bat",
                )
                
                var lastException: Exception = e
                for (fallback in fallbacks) {
                    try {
                        runTauriCli(fallback)
                        return
                    } catch (fallbackException: Exception) {
                        lastException = fallbackException
                    }
                }
                throw lastException
            } else {
                throw e;
            }
        }
    }

    fun runTauriCli(@Suppress("UNUSED_PARAMETER") executable: String) {
        // Tauri's current Android CLI builds the web bundle but does not sync
        // it into this customized generated project. Without this step Gradle
        // packages the stale assets left by the old SMS experiment.
        val frontend = project.projectDir.resolve("../../../../dist").canonicalFile
        if (!frontend.isDirectory) {
            throw GradleException("Expected built frontend at $frontend")
        }
        project.copy {
            from(frontend)
            into(project.file("src/main/assets"))
        }
    }
}
