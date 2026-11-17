# Solicitudes de incorporación y menú de miembros — Plan de implementación

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Separar las solicitudes de incorporación OIDC e invitaciones del directorio de miembros, permitiendo aprobar cada solicitud con un rol y consultar miembros y roles desde Solicitudes.

**Architecture:** El API mantendrá una bandeja de solicitudes de incorporación independiente de `actor_roles`. Las identidades OIDC pendientes y las invitaciones convergerán en el mismo flujo de decisión; aprobar una solicitud asignará el rol y sus capacidades mediante las reglas existentes. La web mostrará una sección de solicitudes y un menú de miembros aprobado dentro del área de Mesa de ayuda, sin exponer el espacio técnico a roles `user`/`supervisor`.

**Tech Stack:** Rust, PostgreSQL, Axum, React, TypeScript, Vitest.

**Spec:** Este documento y los requisitos confirmados en la conversación del 17 de noviembre de 2026.

## Global Constraints

- Trabajar sobre `develop`; no crear ramas `codex/*`.
- Los PR siempre serán de `develop` hacia `main`.
- Una identidad autenticada por OIDC no se convierte automáticamente en miembro activo.
- Los roles de Mesa de ayuda son `user` y `supervisor`; ambos quedan aislados de proyectos, código y tareas técnicas.
- Las invitaciones y las solicitudes OIDC deben terminar en una asignación explícita de rol.
- El login primario ofrece únicamente usuario/contraseña u OIDC; Passkey nunca aparece como método primario.
- El segundo factor se ofrece después de comprobar que la identidad tiene un perfil aprobado.
- El segundo factor permite elegir TOTP o Passkey y omitirlo como máximo cinco veces; el sexto acceso exige configurar o completar un método.
- Mantener una clase, enum o funcionalidad explícita por archivo cuando se creen unidades nuevas.

## Review Focus

- Una identidad OIDC pendiente puede iniciar sesión y consultar su estado, pero no debe aparecer como miembro técnico ni recibir permisos técnicos.
- Una solicitud aprobada con `user` o `supervisor` debe entrar solamente a Mesa de ayuda.
- Una invitación aceptada no debe saltarse la decisión de rol ni duplicar actores o identidades.
- Una decisión repetida o concurrente debe ser idempotente y no cambiar un rol ya aprobado sin una operación explícita.
- Revocar o archivar un miembro debe retirarlo de las listas activas sin borrar la evidencia de la solicitud.
- Un usuario con perfil y segundo factor configurado debe pasar por la pantalla de autenticación correspondiente en cada inicio de sesión.

### Task 1: Modelo de solicitudes de incorporación en API

**Files:**
- Create: migration para solicitudes de incorporación.
- Create: tipos de dominio para solicitud, estado y decisión.
- Modify: registro de migraciones y módulos del store.
- Test: pruebas de validación, estados, idempotencia y concurrencia.

**Interfaces:**
- Produces: endpoints de listado, detalle, decisión y cancelación para solicitudes de incorporación.
- Consumes: `oidc_identities`, `user_security`, invitaciones existentes y `actor_roles`.

- [ ] Escribir pruebas para solicitudes OIDC pendientes, invitaciones pendientes, decisión de rol válida, rechazo, repetición y conflicto de revisión.
- [ ] Ejecutar las pruebas y confirmar que fallan por ausencia del modelo y endpoints.
- [ ] Implementar el modelo persistente y las operaciones transaccionales sin asignar rol al crear la solicitud.
- [ ] Ejecutar las pruebas del store y del API.
- [ ] Commit: `feat(api): add member onboarding requests`.

### Task 2: Unificar OIDC e invitaciones con la bandeja de incorporación

**Files:**
- Modify: flujo de callback OIDC y creación de invitaciones.
- Modify: handlers y permisos de solicitudes.
- Test: pruebas HTTP para ambos orígenes y aislamiento por rol.

**Interfaces:**
- Consumes: modelo de Task 1.
- Produces: solicitudes con origen `oidc` o `invitation`, correo, actor e información de expiración.

- [ ] Escribir pruebas que comprueben que el primer login OIDC crea una solicitud y que una invitación crea otra sin activar el miembro.
- [ ] Implementar la convergencia de ambos flujos, preservando `issuer + subject` como identidad OIDC y evitando confiar en el correo para fusionar cuentas.
- [ ] Implementar aprobación con `user`, `supervisor`, `developer`, `technical_lead`, `pmo` o `manager`, aplicando las capacidades existentes.
- [ ] Implementar rechazo, expiración y reintento seguro.
- [ ] Ejecutar pruebas HTTP y migraciones en una base PostgreSQL aislada.
- [ ] Commit: `feat(api): unify OIDC and invitation onboarding`.

### Task 3: API y navegación web de solicitudes y miembros

**Files:**
- Create: servicios, tipos y parsers para solicitudes de incorporación.
- Create: página de solicitudes y componentes de decisión.
- Create: menú/página de miembros aprobados para Mesa de ayuda.
- Modify: navegación por rol y rutas del workspace.
- Test: pruebas de navegación, visibilidad por rol, decisión y consulta de miembros.

**Interfaces:**
- Consumes: endpoints de Tasks 1 y 2.
- Produces: menú `Solicitudes` con subsección `Solicitudes de incorporación` y `Miembros`, con acciones condicionadas por permisos.

- [ ] Escribir pruebas web para roles `user`, `supervisor`, `developer`, `manager` y `super_admin`.
- [ ] Implementar la bandeja con origen, correo, fecha, estado y acciones permitidas.
- [ ] Implementar modal de aprobación con selector de rol y campos de proyectos solamente para roles técnicos que los necesiten.
- [ ] Implementar el menú de miembros de Mesa de ayuda con nombre, correo, rol, estado y acceso al perfil permitido.
- [ ] Ocultar solicitudes administrativas y miembros técnicos a `user`/`supervisor`.
- [ ] Ejecutar Vitest, lint y build.
- [ ] Commit: `feat(web): add onboarding requests and member menu`.

### Task 4: Contratos, documentación y revisión final

**Files:**
- Modify: `docs/platform/openapi.json`.
- Modify: documentación pública de miembros, OIDC, invitaciones y Mesa de ayuda.
- Test: validación de OpenAPI y pruebas finales API/web.

- [ ] Documentar estados, orígenes, roles permitidos y reglas de aislamiento.
- [ ] Actualizar OpenAPI sin incluir secretos, CI ni detalles privados de despliegue.
- [ ] Ejecutar validación de contratos, pruebas web y API.
- [ ] Revisar `git diff --check`, estado de ambos repositorios y commits acumulados.
- [ ] Preparar PR de `develop` hacia `main` solamente después de autorización explícita.

### Task 5: Restaurar el flujo correcto de segundo factor

**Files:**
- Modify: login web y componentes de autenticación primaria.
- Modify: flujo de `LoginGate` y endpoints/estado de segundo factor en API.
- Modify: páginas existentes de TOTP y componentes Passkey/WebAuthn.
- Test: pruebas de login, selección de factor, cinco omisiones y bloqueo posterior.

**Interfaces:**
- Consumes: estado de perfil aprobado de Task 2 y credenciales Passkey/TOTP existentes.
- Produces: autenticación primaria password/OIDC, seguida de selección y verificación TOTP o Passkey.

- [ ] Escribir una prueba que confirme que la pantalla primaria no contiene un botón Passkey y solo ofrece password/OIDC.
- [ ] Escribir pruebas para usuario pendiente, usuario sin perfil y usuario con perfil aprobado.
- [ ] Escribir pruebas de cinco omisiones permitidas y bloqueo en el sexto intento.
- [ ] Implementar la eliminación del botón Passkey del login primario.
- [ ] Implementar la selección de TOTP o Passkey únicamente después de validar el perfil.
- [ ] Conservar las pantallas TOTP existentes y conectar Passkey al desafío WebAuthn del navegador para abrir el llavero.
- [ ] Ejecutar Vitest, pruebas API, lint y build.
- [ ] Commit: `fix(auth): restore staged second-factor login`.
