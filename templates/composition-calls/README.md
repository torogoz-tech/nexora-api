# Molde con llamadas directas entre regiones

Este catálogo extiende `composition-scopes` con perfiles 0.6.0, composition schema5 y graph10. Se carga por separado; los catálogos anteriores conservan sus contratos.

`direct_calls: true` activa la resolución de llamadas para el perfil. Cada contexto de expresiones debe tener una política de bindings. El modelo mantiene las clases invocables y los operadores que escriben sobre su operando; el motor conserva esa política mediante una identidad de contenido compartida por el grafo.

```sh
cargo run -p nexora-cli -- --language-dir templates/composition-calls analyze templates/composition-calls/fixtures/complete/source.nxb
```

El fixture contiene una llamada a `run` desde una plantilla dentro de una función y otra desde un atributo. Ambas se resuelven al mismo destino del anfitrión mediante sus referencias y puentes de ámbito. El grafo conserva `call_binding`, el sitio original y la política fuente.

Una reasignación conocida al destino o una escritura incierta que pueda alcanzarlo impide certificar la llamada. Los nombres ambiguos, contextos incompletos, valores no declarados invocables y llamadas indirectas o a miembros permanecen explícitos. Los paréntesis se interpretan según la política del modelo.

Es un molde de referencia; no certifica soporte completo de HTML, TypeScript, React ni análisis dinámico. Véase [contrato y límites](../../docs/semantic-understanding/composed-direct-calls.md).
