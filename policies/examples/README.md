# Ejemplo de convención de nombres

EXAMPLE-CLASS-NAMES 1.0.0 demuestra una convención opcional de maintainability de severidad low: clases JavaScript superiores reconocidas usan nombres que empiezan con mayúscula y contienen letras/dígitos ASCII. El propósito es enseñar el motor, no definir cómo debe nombrar clases una organización.

PASS: `class ExampleService {}`. FAIL: `class example_service {}`. UNKNOWN: `export class ExampleService {}`, porque el parser actual no soporta export. La suite naming.tests.yaml verifica los tres casos con el indexador real; el código fuente nunca se ejecuta.

No verifica clases anidadas, templates, métodos ni ECMAScript completo. `target.language` y `target.symbol` delimitan su alcance; onUnknown review conserva incertidumbre sin bloquear CI por defecto.
