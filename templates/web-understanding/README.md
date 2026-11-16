# Catálogo de comprensión interior web

Estos modelos 0.9.0 amplían TypeScript, TSX y JSX con grammar9, graph10 y composition4 sobre el mismo motor. Se seleccionan explícitamente; `languages/` conserva los perfiles predeterminados.

Se interpretan inicializadores compatibles, cuerpos de funciones, funciones anidadas, declaraciones locales `let`/`const`, bloques, expresiones de retorno y firmas compatibles de funciones/arrows. Los parámetros simples se proyectan como `Parameter` en el ámbito de su función. Las anotaciones de tipo admitidas se conservan opacas; reconocer su envoltorio no demuestra comprensión del tipo. Genéricos, defaults, rest, desestructuración y formas pendientes mantienen cobertura parcial/opaca.

Las expresiones conservan operandos, grupos, operadores, miembros y sitios de llamada. Las interpolaciones `${…}` y las expresiones `{…}` de JSX tienen unidades propias, rangos originales y relaciones `INTERPRETS`. Las llaves de objetos/bloques no se confunden con interpolaciones. Las cabeceras JSX siguen sin una gramática completa de atributos.

La composición permite explícitamente buscar bindings en el ámbito contenedor. Cuando los contextos relevantes están completos, una interpolación puede enlazar con su parámetro real incluso con homónimos anidados. La identidad léxica no certifica inicialización ni ejecución: no se habilita resolución de destinos de llamadas. Una anotación o una forma desconocida continúa siendo barrera para vínculos confiados.

Grammar6 ofrece `stop_on_error` por contexto para evitar recuperar nombres de tipos como parámetros, y `gap_without.characters` para expresar restricciones entre tokens significativos, incluidos comentarios. Los modelos usan esta última para las fronteras restringidas de `async`/`=>`. La inserción automática de punto y coma y todos los terminadores en otras construcciones siguen pendientes; los scopes no representan un sistema completo de tipos, namespaces o ejecución.

```sh
cargo run -p nexora-cli -- --language-dir templates/web-understanding analyze templates/web-understanding/fixtures/parameters/source.ts
```

Mediante Rust se requiere `Indexer::with_catalog(language, vec![])` o `CompositionAnalyzer::new(language, vec![])`: el catálogo explícito admite rutas al propio perfil. El CLI prepara ese catálogo.

Evidencia: [firmas y bindings](../../docs/semantic-understanding/web-parameter-bindings.md), [interpolaciones](../../docs/semantic-understanding/web-interpolation-contexts.md), [presupuesto real](../../docs/semantic-understanding/composition-work-accounting.md), [negación prefija](../../docs/semantic-understanding/prefix-negation-model.md). Los reportes históricos conservan las versiones y los límites observados en su etapa.

## Model0.8: index access

The three profiles use grammar8 and declare `[]` with `expression_kind: index`, maximum1 argument and an explicit Index assignment target. Lexer and graph mappings remain unchanged. Index access emits an Index shape rather than a CALLS edge. An actual invocation of an indexed value retains an unresolved target unless proven separately.

Comma expressions inside indices remain unsupported/Partial, preserving neighbors; they are not silently treated as multiple dimensions. Types, casts, conditionals, signatures and broader framework semantics remain incomplete. See [generic contract and verification](../../docs/semantic-understanding/index-expressions.md). Historical measurements retain their original versions/fingerprints.

## Modelo0.9: condicionales

Los tres perfiles declaran `?:` en grammar9, precedencia20 y mínimos10 para ambas ramas, incluidas asignaciones compatibles. La forma `Conditional` conserva condición y dos ramas; no demuestra qué rama se ejecuta. Los carriers JSX y sus expresiones embebidas mantienen su procedencia. El lexer y las proyecciones de grafo permanecen iguales. [Contrato genérico y límites](../../docs/semantic-understanding/conditional-expressions.md).
