namespace Api.Controllers;
[Route("api/[controller]")]
class UsersController : ControllerBase {
    [HttpGet("{id}")] public object Get(int id) => new();
}
