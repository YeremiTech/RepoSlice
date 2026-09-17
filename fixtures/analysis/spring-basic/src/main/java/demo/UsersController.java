package demo;
import org.springframework.web.bind.annotation.*;
@RestController @RequestMapping("/api/users")
class UsersController { @GetMapping("/{id}") public String show(){ return "ok"; } }
