<?php
namespace App\Controller;
use Symfony\Component\Routing\Attribute\Route;
class UserController extends AbstractController {
    #[Route('/users/{id}', name: 'users_show', methods: ['GET'])]
    public function show(int $id): Response {}
}
