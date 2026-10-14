<?php
use App\Http\Controllers\UserController;
Route::get('/users/{user}', [UserController::class, 'show'])->name('users.show');
