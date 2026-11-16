# Molde funcional de un paquete Nexora

Copiar esta carpeta a languages/<id> y editar sus cuatro YAML. El paquete actual tiene ID template y extensión nxexample; usa una sintaxis didáctica que demuestra que el motor no exige formas JS o Lua:

```text
USE infrastructure.storage;
DEF total [order] BEGIN order FINISH
```

## Especializar el molde

1. language.yaml: definir ID/versión/extensiones y cómo los nombres de referencias se transforman en candidatos dentro del snapshot.
2. lexer.yaml: definir todos los tokens/trivia/keywords/literales del subconjunto; los regex son patrones, no código ejecutable.
3. grammar.yaml: expresar la sintaxis mediante operadores comunes, delimitadores, reglas, repeticiones, capturas y proyección. Reemplazar la gramática completa del ejemplo; renombrar keywords por sí solo no implementa otro lenguaje.
4. graph.yaml: mapear los hechos reconocidos al vocabulario canónico. Declarar solamente las relaciones que el paquete extrae realmente.
5. Crear fixtures válidos, inválidos y parcialmente soportados; verificar rangos, grafo, resolución, cobertura, límites y políticas neutrales. Medir desempeño con corpus representativo.

```bash
cargo run -p nexora-cli -- languages validate templates/language-pack
cargo run -p nexora-cli -- --language-dir templates/language-pack analyze tests/fixtures/template-repository/domain/order.nxexample
cargo run -p nexora-cli -- --language-dir templates/language-pack governance check tests/fixtures/template-repository --policies policies/architecture/language-neutral-boundary/boundary.policy.yaml
```

El último comando devuelve FAIL y salida 1 porque la referencia se resuelve a modules/infrastructure/storage.nxexample. El paquete se descubre automáticamente al copiarse al catálogo con un ID único. Si comparte extensiones con otro paquete, seleccionar --language <id> explícitamente. No se añade ningún archivo Rust ni handler por lenguaje.

Contrato y schemas: [guía](../../docs/language-packs/README.md), [gramática v2](../../schemas/grammar-v2.schema.json), [manifest](../../schemas/language-v1.schema.json), [lexer](../../schemas/lexer-v1.schema.json), [grafo v2](../../schemas/graph-v2.schema.json). Las validaciones cruzadas y los límites adicionales se ejecutan en el compilador del paquete.

El molde también reconoce `METHOD Receiver.run [] BEGIN FINISH` y `PUBLISH Receiver.run;`: sus reglas y referencias locales se describen en [proyección de símbolos](../../docs/symbol-projection/README.md). No hay un handler Rust para estas palabras.
