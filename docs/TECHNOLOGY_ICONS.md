# Iconos de tecnologías

Los logotipos utilizados por la aplicación de escritorio son archivos PNG locales en `apps/desktop/public/assets/technologies/`. Las carpetas corresponden a **languages**, **frameworks**, **databases**, **orm**, **tooling** y **runtime**. No se cargan imágenes desde Internet, tanto para proyectos locales como para los obtenidos de GitHub.

La relación entre el nombre devuelto por el analizador y el archivo se define exclusivamente en `apps/desktop/src/lib/assets.json`. Cada entrada incluye `slug`, `category`, `path`, `aliases` y `specific`. Agregar un PNG sin registrar la entrada o actualizar la ruta de una entrada existente no cambia lo que se muestra. Los alias deben ser equivalencias explícitas del mismo producto; un icono **no implica** que el analizador detecte automáticamente esa tecnología.

Al incorporar imágenes nuevas:

1. Colocar el PNG en la categoría apropiada, con un nombre de archivo correcto y único.
2. Registrar el PNG y sus alias en `assets.json` (o sustituir la ruta `fallback/...` de la entrada ya existente). Marcar `specific: true` exclusivamente cuando se dispone de un logotipo propio.
3. Si se necesita **detectar** una tecnología todavía no soportada, incorporar evidencias verificables en el analizador por separado; no inferir su presencia por el nombre de una carpeta ni por disponer del icono.
4. Ejecutar desde `apps/desktop` `npm test` y `npm run build`. Las pruebas impiden incorporar imágenes sin registrar, rutas inexistentes, PNG inválidos y alias ambiguos.

La resolución de iconos es sensible a la categoría cuando un producto tiene más de un rol; por ejemplo, Java dispone de una imagen para lenguaje y otra para entorno de ejecución. Si el nombre no está registrado, o falla la carga del PNG, la interfaz usa el icono genérico de su categoría. La lista `docs/png-pending.json` contiene solo las tecnologías que todavía necesitan logotipos propios.

Los 124 PNG de tecnologías distribuidos en este paquete están optimizados a un máximo de **512 × 512 píxeles** para su uso en iconos de interfaz de 28–52 píxeles. Esto reduce el tamaño del paquete y la memoria necesaria para decodificarlos, sin modificar el código del motor de análisis.
