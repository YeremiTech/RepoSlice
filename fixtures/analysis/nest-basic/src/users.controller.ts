import { Controller, Get, Post } from '@nestjs/common';
@Controller('users')
export class UsersController {
  @Get(':id') findOne() {}
  @Post() create() {}
}
