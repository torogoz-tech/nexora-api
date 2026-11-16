# Molde con vínculos explícitos entre ámbitos

Este catálogo extiende `composition-markup` con composition schema4, graph9 y perfiles 0.5.0. Se carga por separado: conserva los mismos identificadores lógicos de los moldes anteriores, que siguen aislados.

`scope.parent_lookup: true` en una ruta permite consultar el ámbito contenedor. Una ruta con `false` detiene esa búsqueda. En una cabecera con expresión anidada, ambas rutas deben permitir el recorrido para llegar al anfitrión. Las declaraciones locales conservan las políticas `nearest`/`collect` y de visibilidad del modelo. No se exportan nombres automáticamente a anfitriones o regiones hermanas.

El fixture combina una función y un parámetro, una plantilla interpolada, una cabecera con una expresión y una lectura del anfitrión. Sus cuatro referencias se resuelven; el grafo conserva el sitio original de cada referencia y los vínculos `SCOPE_BRIDGE` que permiten la consulta.

```sh
cargo run -p nexora-cli -- --language-dir templates/composition-scopes analyze templates/composition-scopes/fixtures/complete/source.nxb
```

Es una gramática de referencia acotada, no soporte completo de HTML, TypeScript o React. Las llamadas a declaraciones de otra unidad todavía se publican sin resolver, con motivo explícito. Véase [contrato y límites](../../docs/semantic-understanding/composed-scope-bindings.md).
