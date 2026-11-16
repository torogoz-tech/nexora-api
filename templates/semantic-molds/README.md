# Moldes de semántica estructural

Estos dos paquetes didácticos describen jerarquía, regiones, ámbitos y expresiones con la misma implementación. Ninguno representa un lenguaje de producción completo.

- brace_mold: FUNC / VAR / BLOCK / READ, paréntesis y llaves.
- keyword_mold: DEF / LET / SCOPE / USE_LOCAL, corchetes y BEGIN/FINISH.

Cada carpeta contiene language.yaml, lexer.yaml, grammar.yaml (schema 4), graph.yaml (schema 4) y ejemplos nested/expressions. La versión 0.2.0 añade EVAL/VALUE, inicializadores y tablas de operadores. Cambiar sintaxis, precedencia y políticas se hace en esos datos. Ver [contrato y uso](../../docs/semantic-understanding/bodies-and-scopes.md).

Ver también [expresiones y límites](../../docs/semantic-understanding/expressions.md).
