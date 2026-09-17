<?php
namespace App\Http\Controllers;
use App\Models\User;
class UserController extends Controller {
    public function __construct(User $user) {}
    public function show(User $user) { return $user; }
}
