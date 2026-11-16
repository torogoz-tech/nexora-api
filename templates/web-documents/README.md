# Documentos HTML con JavaScript y CSS embebidos

Catálogo optativo y autocontenido sobre el motor común 0.22.0. Incluye HTML 0.3.0, JavaScript 0.1.0 y CSS 0.3.0, cada uno con cuatro modelos YAML. No añade ramas al lexer/parser Rust ni dependencias de un navegador o compilador.

```sh
cargo run -p nexora-cli -- --language-dir templates/web-documents analyze templates/web-documents/fixtures/mixed/source.html
cargo run -p nexora-cli -- --language-dir templates/web-documents index templates/web-documents/fixtures/mixed
```

El perfil HTML declara regiones para script/style y rutas hacia contextos de JavaScript/CSS. JavaScript conserva sus propias interpolaciones de plantilla y parámetros; CSS conserva propiedades, valores y componentes; selectores, URLs y semántica de estilo siguen con incertidumbre explícita. Los offsets se refieren siempre al documento original.

Las cabeceras conocidas de scripts inline aceptan el tipo predeterminado, algunos MIME JavaScript reconocidos y module. Otros tipos, atributos src/language y combinaciones de type no cubiertas permanecen opacos. No se buscan recursos externos. Las reglas conservan atributos entre comillas, distinción ASCII de nombres y límites de etiquetas; comentarios y contenedores de texto no generan declaraciones JavaScript.

Es un modelo parcial, sin DOM, ejecución, semántica CSS completa ni todos los estados del tokenizer HTML. Los scripts se interpretan con un subconjunto compartido y ámbitos aislados: no se infieren globals compartidos entre scripts clásicos ni toda la semántica clásica/module. SVG, MathML, noscript y los escapes históricos de script mediante comentarios se rechazan explícitamente hasta modelarlos. Los casos de contenido/tipos desconocidos mantienen cobertura Opaque/Partial.

API Rust: cargar los tres perfiles y usar `CompositionAnalyzer::new(html, vec![javascript, css])` o `Indexer::with_catalog` con el mismo catálogo explícito. Los perfiles predeterminados de `languages/` no cambian.

[Requisitos, evidencia y limitaciones](../../docs/semantic-understanding/html-embedded-languages.md). [Reporte público del fixture](../../docs/experiments/web-documents/fixture-report.json).

HTML 0.3.0 declara plaintext como [región literal hasta el final de la entrada](../../docs/semantic-understanding/eof-only-regions.md). [Fixture y evidencia actuales](../../docs/experiments/web-documents/eof-region-report.json). El reporte inicial del documento mixto conserva su versión histórica 0.20.0.

CSS 0.3.0 incorpora [contextos de declaraciones y componentes](../../docs/semantic-understanding/css-declaration-contexts.md). La calibración inicial de [CDCA](../../docs/experiments/cdca/README.md) conserva 32/32 grafos CSS y 15.011 declaraciones sin ejecutar la aplicación.
