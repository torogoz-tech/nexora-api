# Molde de marcado con cabeceras y expresiones

Este catálogo reutiliza el motor común para interpretar cabeceras y cuerpos por separado. Sus perfiles `region_carrier` y `region_expression` 0.4.0 se cargan juntos y por separado de los catálogos anteriores, cuyos identificadores coinciden. Los archivos YAML contienen toda la especialización; no hay código Rust específico para estos nombres.

El perfil anfitrión reconoce funciones `FUNC`, bloques `BLOCK`, declaraciones `VAR`, lecturas `READ` y marcado anidado con cierres por nombre. Las cabeceras admiten atributos `nombre="texto"` o `nombre={expresión}`. Cada cabecera se interpreta con el contexto `attributes` del perfil de expresiones; cada expresión entre llaves tiene su propia interpretación. Los cuerpos se declaran portadores de texto literal y marcado hijo.

```text
VAR host;
<Box count={(1 + 2)} title="café"><Leaf title="nested"/></Box>
READ host;
```

Desde la raíz del workspace:

```sh
cargo run -p nexora-cli -- --language-dir templates/composition-markup analyze templates/composition-markup/fixtures/complete/source.nxb
```

El resultado conserva las unidades interiores, expresiones con sus operandos, posiciones originales, pruebas de cobertura y relaciones `INTERPRETS` identificadas como `header` o `content`. La lectura `READ host` se resuelve en el anfitrión. Los nombres de atributos son contenedores; no declaran variables ni bindings de componentes. Las expresiones interiores no reciben acceso implícito a los ámbitos del anfitrión.

Es un molde de referencia acotado, no una gramática completa de HTML, TSX o React. No admite atributos booleanos, spreads, comillas simples ni reglas de componentes/frameworks. `Parsed` certifica el contrato declarado del molde. Los modelos web nativos necesitan sus propias gramáticas interiores antes de activar rutas equivalentes.

Las pruebas de integración comprueban autocierre, Unicode, truncamiento independiente, expresiones reales, guardas cubiertas por cabeceras, corrupción de evidencia, equivalencia de snapshots fríos/calientes y edición incremental. Véase [documentación del motor](../../docs/semantic-understanding/mode-token-contexts.md).
