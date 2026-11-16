# Infraestructura local de Nexora

Este stack aislado incorpora PostgreSQL con pgvector, Dragonfly y Neo4j Community. La imagen `pgvector/pgvector:pg16` permite aplicar el índice semántico del RAG junto al almacén canónico. No se reutilizan volúmenes, bases ni credenciales de otros proyectos. Docker Compose no administra ni reinicia esos otros proyectos.

## Arranque

Desde la raíz de Nexora:

```sh
python3 infra/local/prepare.py
docker compose -f infra/local/compose.yaml up -d --wait --wait-timeout 180
python3 infra/local/verify.py --recreate
uv run infra/local/verify_bolt.py
```

`prepare.py` genera credenciales aleatorias sólo cuando faltan y conserva las existentes. Se guardan en `.nexora/local/secrets/`, excluido por `.gitignore`, con permisos restringidos. No se imprimen. El directorio de secretos tiene modo0700; los archivos PostgreSQL/Dragonfly modo0600. El archivo Neo4j tiene modo0644 dentro de ese directorio privado porque el contenedor lo lee después de abandonar root; no es accesible por otros usuarios del host atravesando ese directorio. Compose las monta como archivos de secretos, no incorpora sus valores a este YAML. No borrar esos archivos para cambiar una contraseña: las bases persistidas conservan la contraseña anterior y requieren su procedimiento de rotación.

| Servicio | Imagen | Acceso desde el host | Acceso dentro de la red knowledge |
|---|---|---|---|
| PostgreSQL | pgvector/pgvector:pg16 | 127.0.0.1:15432 | postgres:5432 |
| Dragonfly | ghcr.io/dragonflydb/dragonfly:v2.0.2 | 127.0.0.1:16379 | dragonfly:6379 |
| Neo4j Community | neo4j:5.26.32 | HTTP 127.0.0.1:17474; Bolt 127.0.0.1:17687 | neo4j:7474; neo4j:7687 |

Base/usuario PostgreSQL: `nexora`/`nexora`. Usuario Neo4j: `neo4j`; base: `neo4j`. Credenciales: archivos privados `postgres_password`, `neo4j_auth` (`usuario/contraseña`) y `dragonfly_flags` (incluye requirepass). Los puertos se pueden sobrescribir con `NEXORA_POSTGRES_PORT`, `NEXORA_DRAGONFLY_PORT`, `NEXORA_NEO4J_HTTP_PORT` y `NEXORA_NEO4J_BOLT_PORT`; mantener las mismas variables al verificar.

## Persistencia y límites

Los volúmenes `nexora-local_postgres_data`, `nexora-local_dragonfly_data`, `nexora-local_neo4j_data` y `nexora-local_neo4j_logs` sobreviven a recrear contenedores. `docker compose ... down` conserva volúmenes; `down -v` los elimina y no forma parte del flujo normal.

Dragonfly configura `/data`, `dbfilename=nexora` y un snapshot cada minuto. También guarda al apagarse ordenadamente. No ofrece AOF: ante caída abrupta puede perder cambios posteriores al último snapshot. El probe usa un cliente RESP2 mínimo en Python para detectar errores del servidor independientemente de la versión de redis-cli de la imagen. Ejecuta SAVE antes de recrear y comprueba recuperación desde ese checkpoint; no demuestra cero pérdida de datos ni alta disponibilidad. La persistencia del caché es una optimización, no reemplaza PostgreSQL como fuente durable de conocimiento.

`cache_mode=false` evita activar expulsión automática por presión de memoria en Dragonfly. La validez y futura caducidad de cada entrada deben pertenecer al motor: identidad de contenido, contexto, perfil, versión y snapshot. La infraestructura habilita expiración por clave; el adaptador y la política del motor se describen en [persistencia y caché](../../docs/storage-runtime.md). Al agotar memoria deben manejarse errores, no prometer caché ilimitado.

Límites locales: PostgreSQL256MiB/0.5CPU; Dragonfly384MiB/0.5CPU (maxmemory256MiB,1hilo); Neo4j768MiB/1CPU (heap128–256MiB,page cache64MiB). Son valores para desarrollar en el Docker local de4GiB, no dimensionamiento de producción. Dragonfly usa epoll si io_uring no está disponible. Se requieren disco libre en la VM Docker y margen de memoria para los otros proyectos.

## Verificación reproducible

`verify.py` realiza operaciones autenticadas sobre datos sintéticos propios: fila PostgreSQL, clave Dragonfly y dos nodos unidos por una relación Neo4j. Comprueba lectura y recorrido, expiración por clave y SAVE. Con `--recreate` permite hasta360s para apagado, recreación y espera de salud (hasta180s para esta última), sin convertir ese límite en una promesa de latencia de consulta. Sustituye las identidades de los tres contenedores conservando volúmenes y vuelve a consultar los mismos datos. Finalmente elimina sólo sus propios registros de prueba y guarda otra vez el caché. No carga código de los repositorios privados ni aplica migraciones del motor.

Los healthchecks comprueban disponibilidad (pg_isready, PING y disponibilidad HTTP Neo4j, sin arrancar otra JVM); el probe comprueba adicionalmente autenticación, escritura/lectura y persistencia. Si falla, imprime un error sin credenciales y puede dejar registros sintéticos identificados con `nexora_infra_probe`, `nexora:infra-probe:*` o `NexoraInfraProbe`; nunca ejecutar limpieza general de las bases para retirarlos.

## Integración del RAG

Arrancar los contenedores prepara los servicios. Aplicar las migraciones del
almacén de plataforma con:

```sh
cargo run -p nexora-api -- --connections .nexora/local/connections.json --schema nexora_platform migrate
```

La comprobación local del 15 de noviembre de 2026 confirmó PostgreSQL 16.16,
pgvector 0.8.7 y las versiones de plataforma 110–113, incluyendo las tablas de
jobs, embeddings de conocimiento, embeddings de snapshots e índices HNSW de
1536 dimensiones. La conexión del motor, la publicación y las consultas se
describen en [persistencia y caché](../../docs/storage-runtime.md).

La futura publicación entre PostgreSQL y Neo4j debe usar snapshots versionados y un estado de proyección explícito: MCP no debe consultar un grafo incompleto ni mezclar versiones. TTL no sustituye invalidación por cambio de código/plantilla, y la caché no decide qué snapshot está publicado.

## Fuentes de configuración

- [Neo4j Docker y secretos](https://neo4j.com/docs/operations-manual/current/docker/docker-compose-standalone/)
- [Neo4j configuración de memoria](https://neo4j.com/docs/operations-manual/current/docker/configuration/)
- [Dragonfly snapshots y recuperación](https://www.dragonflydb.io/docs/managing-dragonfly/backups)
- [Dragonfly: sin AOF](https://www.dragonflydb.io/docs/managing-dragonfly/aof)

## Última comprobación

La configuración final pasó `python3 infra/local/verify.py --recreate`: operaciones autenticadas, TTL, snapshot y recuperación de datos con nuevas identidades de los tres contenedores. Se retiraron los registros de prueba, incluidos5 de ejecuciones interrumpidas. La comprobación final de salud, imágenes, volúmenes y puertos está en [verified.json](verified.json). El perfil Neo4j conserva heap máximo256MiB; un experimento con192MiB presentó timeouts de primera consulta. Estos resultados no son una medición de rendimiento y el arranque en frío puede tardar varios minutos en la VM compartida.

## Neo4j Browser y descubrimiento Bolt

Abrir http://127.0.0.1:17474/browser/ y conectar con `neo4j://127.0.0.1:17687` o `bolt://127.0.0.1:17687`. Usuario `neo4j`, contraseña del archivo privado. Si el navegador guardó la dirección anterior de7687, desconectar y reemplazarla; recargar después de recrear el servicio.

El puerto interno7687 se publica como17687. Compose declara `server.bolt.advertised_address` y `server.http.advertised_address` con los puertos externos, de modo que el descubrimiento del navegador no propone puertos inaccesibles del contenedor. La escucha interna permanece en7687/7474. `NEXORA_NEO4J_ADVERTISED_HOST` permite seleccionar otro host alcanzable; su valor por defecto es127.0.0.1 para este stack local. Clientes dentro de Docker pueden usar una conexión directa `bolt://neo4j:7687`; el routing anunciado está configurado para clientes del host.

`uv run infra/local/verify_bolt.py` comprueba las dos URLs anunciadas, conexión Bolt directa y con routing, una consulta en la base neo4j y descubrimiento en system. Usa un entorno temporal con el driver oficial, sin añadir dependencias al motor Rust ni imprimir credenciales. Complementa la prueba HTTP; una comprobación HTTP exitosa por sí sola no valida el acceso Bolt del navegador.
