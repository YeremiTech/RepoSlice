<?php
Route::prefix('admin')->middleware(['auth', 'verified'])->name('admin.')->group(function () {
    Route::post('/reports', function () {})->name('reports.store');
});
