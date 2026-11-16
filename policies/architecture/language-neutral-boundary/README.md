# Regla neutral de frontera arquitectónica

EXAMPLE-LANGUAGE-NEUTRAL-BOUNDARY 1.0.0 es opcional, de architecture y severidad high. Selecciona File bajo domain/ y prohíbe DEPENDS_ON directo hacia infrastructure/ o modules/infrastructure/. No contiene target.language ni sintaxis de fuente.

La misma política produce PASS/FAIL/UNKNOWN en fixtures JavaScript y Lua, con un mismo hash del conjunto de políticas. La suite declara el idioma de cada caso mediante language y usa el catálogo de paquetes. El molde didáctico también produce un FAIL real sobre esta política, sin cambiar el evaluador.

```bash
cargo run -p nexora-cli -- policy test policies/architecture/language-neutral-boundary
```

La regla comprueba hechos reconocidos y cobertura del paquete; no certifica todas las dependencias de runtime. onUnknown fail bloquea CI ante evidencia insuficiente. No es un estándar universal de arquitectura.
