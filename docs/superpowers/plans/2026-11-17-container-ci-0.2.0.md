# Nexora 0.2.0 Container CI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Prepare the independent API and web repositories for version 0.2.0 with reproducible container builds, Kubernetes deployment manifests, and GitHub Actions limited to CI.

**Architecture:** The API and web repositories own their source containers and CI workflows, while the common `nexora` documentation repository owns Kubernetes manifests and deployment documentation. Each CI workflow compiles or builds the application once, uploads that output as a workflow artifact, and uses a later image job to download the artifact and assemble the runtime image without compiling again. Kubernetes manifests remain deployment inputs for the operator and contain no environment-specific secrets or CD automation.

**Tech Stack:** Rust 1.100/Cargo, React/Vite/Node, Docker BuildKit, Kubernetes YAML, GitHub Actions.

**Spec:** User request in the conversation: prepare API and web for containers and Kubernetes; GitHub pipelines perform CI only; deployment/CD is performed separately by the operator on a private Docker server.

## Global Constraints

- Release version is `0.2.0` in both repositories and must remain aligned.
- Container and workflow artifact versioning follows the product SemVer release, currently `0.2.0`; floating tags are not used.
- GitHub Actions may publish the versioned runtime images to GHCR, but must not access the deployment server or apply Kubernetes resources.
- Images must build from the CI-produced artifact without embedding credentials, private URLs, or environment-specific configuration.
- Kubernetes manifests must use externally supplied Secrets/ConfigMaps and must not commit secret values.
- Preserve repository independence: API and web changes stay in their own repositories; shared Kubernetes artifacts and operational documentation belong in the `nexora` documentation repository.
- Follow the repository's one-responsibility-per-file and SOLID conventions where source code is touched.

## Review Focus

- The API container must listen on the container interface rather than loopback; CI validates the image command and manifest port.
- The web container must serve SPA fallback routes; CI builds the static output and validates the server configuration.
- Images must run as non-root users and have usable health checks without credentials.
- Kubernetes configuration must fail safely when required external configuration is absent rather than supplying fake production values.
- CI must validate both manifests and Docker builds without performing deployment or registry publication.

### Task 1: API release metadata and container

**Files:**
- Modify: `Cargo.toml`
- Modify: `VERSION`
- Modify: `README.md`
- Create: `Dockerfile`
- Create: `.dockerignore`

- [ ] Update the workspace version and release boundary to `0.2.0`.
- [ ] Add a multi-stage API image using the locked Rust toolchain, a minimal runtime, a non-root user, `0.0.0.0:18080` binding, and a container health check.
- [ ] Exclude build output, credentials, local configuration, and VCS metadata from the build context.
- [ ] Verify the image can be built without external services.

### Task 2: API CI artifact and container workflow

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] Keep the existing API quality checks and add one compilation job that produces the release binary once.
- [ ] Upload the binary as a workflow artifact named with the `0.2.0` release version and a retention policy.
- [ ] Add a dependent image job that downloads the artifact and builds the runtime image locally from that artifact only, using the `0.2.0` image tag.
- [ ] Publish the versioned image to GHCR only from `main`, using `NUGET_TOKEN`; do not publish from pull requests or deploy to infrastructure.

### Task 3: Web release metadata and container

**Files:**
- Modify: `package.json`
- Modify: `package-lock.json`
- Modify: `VERSION`
- Modify: `README.md`
- Create: `Dockerfile`
- Create: `nginx.conf`
- Create: `.dockerignore`

- [ ] Update the web version to `0.2.0`.
- [ ] Add a multi-stage runtime image assembled from a downloaded Vite artifact, with an unprivileged static server and SPA fallback.
- [ ] Keep the API origin configurable at deployment time through the reverse proxy/configuration boundary.
- [ ] Verify dependency lock consistency and production build behavior.

### Task 4: Web CI artifact and container workflow

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] Run lockfile install, tests, lint, and the Vite build once.
- [ ] Upload the generated `dist` directory as a workflow artifact named with the `0.2.0` release version.
- [ ] Add a dependent image job that downloads `dist` and builds the runtime image without rerunning npm or Vite, using the `0.2.0` image tag.
- [ ] Keep CI free of server and cluster credentials; use only `NUGET_TOKEN` for GHCR publication.

### Task 5: Shared Kubernetes manifests and deployment documentation

**Files:**
- Modify: `/Users/abner/Projects/nexora/docs/deployment-architecture.md`
- Create: `/Users/abner/Projects/nexora/deployment/kubernetes/base/kustomization.yaml`
- Create: `/Users/abner/Projects/nexora/deployment/kubernetes/base/api-deployment.yaml`
- Create: `/Users/abner/Projects/nexora/deployment/kubernetes/base/api-service.yaml`
- Create: `/Users/abner/Projects/nexora/deployment/kubernetes/base/web-deployment.yaml`
- Create: `/Users/abner/Projects/nexora/deployment/kubernetes/base/web-service.yaml`
- Create: `/Users/abner/Projects/nexora/deployment/kubernetes/base/configuration.example.yaml`
- Create: `/Users/abner/Projects/nexora/deployment/kubernetes/README.md`

- [ ] Define API and web Deployments and ClusterIP Services with resource bounds, probes, non-root security contexts, and configurable replicas.
- [ ] Reference image names using the operator-supplied `0.2.0` release tag, plus external configuration and Secret names without committing environment values.
- [ ] Document the operator-owned CD sequence for a private Docker server and Kubernetes cluster, including migrations, routing, secrets, storage, backups, and rollback.
- [ ] Add a documentation-repository CI workflow that renders and validates the manifests without applying them.

### Task 6: Cross-repository verification and commits

- [ ] Run the documented API and web checks, artifact-based Docker builds, and Kubernetes rendering/validation.
- [ ] Inspect the final diffs for secrets, deployment side effects, and version alignment.
- [ ] Commit all implementation changes in each repository with a release-preparation message.

