<?php
return new class extends Migration {
    public function up(): void {
        Schema::create('posts', function (Blueprint $table) {
            $table->foreignId('user_id')->constrained('users');
        });
    }
};
