# Moldes de composición

`region_carrier` declara la ruta del modo `code` a `region_expression/expression`. El segundo perfil declara la misma ruta para plantillas anidadas. Se proporcionan ambos explícitamente a CompositionAnalyzer; no se buscan dependencias en red ni por extensión. Las fronteras usan `~[ ... ]~` y `@( ... )`, con paréntesis/comillas anidados.

`nested.nxb` demuestra cambio de gramática y procedencia de flujos. Las políticas son aisladas: `outside` dentro de la expresión no se vincula al símbolo del anfitrión. Las referencias interiores desconocidas se mantienen sin resolver. Cada grafo se consulta con el ID de unidad; RegionInterpretation enlaza el nodo de región de una unidad con la interpretación correspondiente. El texto de plantilla no se presenta como semánticamente comprendido.

Estos moldes prueban el contrato común y no son perfiles completos de ningún lenguaje de producción. Se integran con Indexer::with_catalog y con CLI analyze/index/index-session/governance/policy test. Los puentes de ámbitos siguen pendientes. El perfil anfitrión 0.2 declara .nxb; el interior conserva .nxbc como extensión propia, aunque su uso embebido se selecciona por ruta, no por extensión.

`fixtures/clean/source.nxb` permite comprobar reutilización de caché sin diagnósticos. Véase docs/semantic-understanding/composed-indexing.md para los informes y límites.
