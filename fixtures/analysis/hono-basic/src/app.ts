import { Hono } from 'hono'; const app = new Hono().basePath('/api'); app.get('/health', c => c.text('ok'))
