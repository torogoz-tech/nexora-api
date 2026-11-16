# Moldes con cobertura explícita

Estos perfiles extienden los moldes de composición con schema2 y roles por modo: template=carrier, code=grammar, paren=guard, quote=literal. Los tramos intermedios de una portadora se declaran texto literal; cada región hija conserva su propia prueba. Un guard sólo se cubre mediante una interpretación gramatical que incluya su span completo. Un modo sin rol sigue Opaque.

La selección es declarativa y no contiene condiciones por lenguaje en el motor. Los identificadores coinciden con los moldes anteriores, pero este catálogo se carga por separado y sus versiones son distintas. `fixtures/complete/source.nxb` prueba anidamiento, paréntesis, cadenas y referencias locales del anfitrión. No hay acceso implícito entre ámbitos ni inferencia de tipos. Parsed significa cobertura del contrato de este molde; no certifica semántica completa de HTML, TSX, atributos u otro lenguaje de producción.

Los moldes anteriores en composition-molds mantienen schema1 y sus regiones opacas. Los perfiles web todavía requieren gramáticas interiores antes de recibir estos roles. Véase docs/semantic-understanding/region-coverage.md.
