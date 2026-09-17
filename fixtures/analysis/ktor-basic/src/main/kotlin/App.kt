import io.ktor.server.routing.*
fun routes() { routing { route("/api") { get("/health") {} } } }
