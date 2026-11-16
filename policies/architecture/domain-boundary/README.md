# EXAMPLE-DOMAIN-BOUNDARY 1.0.0

Convención opcional de architecture, severidad high. Prohíbe dependencias estáticas locales directas de archivos domain/ hacia infrastructure/; no se presenta como una norma universal. El propósito es demostrar una frontera arquitectónica sobre importaciones reales.

- PASS: domain/order.js importa shared/value.js, o no tiene imports y su análisis es completo.
- FAIL: importa infrastructure/storage.js. Un import externo adicional no elimina ese incumplimiento confirmado.
- UNKNOWN: paquete externo, destino faltante o ambiguo, import dinámico o loader usado mediante alias. onUnknown fail bloquea CI por defecto.

La suite domain-boundary.tests.yaml ejecuta nueve repositorios pequeños con el indexador real. No se ejecutan sus fuentes. La comprobación es por archivos, a profundidad 1; no verifica símbolos exportados, reexports ni toda la semántica/runtime JavaScript. Ver [alcance y límites](../../../docs/module-dependencies.md).
