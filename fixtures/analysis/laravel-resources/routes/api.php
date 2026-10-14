<?php
Route::apiResource('users', UserController::class)->only(['index', 'show']);
Route::resource('posts.comments', CommentController::class)->except(['destroy']);
