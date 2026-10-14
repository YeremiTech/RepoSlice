var builder = WebApplication.CreateBuilder(args);
var app = builder.Build();
app.MapGet("/health", () => "ok");
app.MapPost("/users", () => Results.Ok());
app.Run();
