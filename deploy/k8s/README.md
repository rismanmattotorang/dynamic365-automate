# Deploying D365-Automate on Kubernetes

These manifests deploy the **read-only** D365-Automate server tier.
The write-enabled gateway tier (Teams / Slack / Telegram, with
`--enable-writes`) is intentionally kept in a separate namespace so
that compromising the read tier cannot escalate to writes.

## Files

| File | Purpose |
|---|---|
| `namespace.yaml`            | Creates the `d365-automate` namespace. |
| `configmap.yaml`            | Non-sensitive config + AGENTS.md guardrails. |
| `secret.example.yaml`       | Template only — populate via Vault / External Secrets / Sealed Secrets. **Do not commit a real secret.** |
| `deployment.yaml`           | 3-replica server Deployment with distroless image, nonroot UID, read-only rootfs, dropped capabilities, hardened probes, topology spread. |
| `service.yaml`              | ClusterIP service with ClientIP session affinity (1 h). |
| `hpa.yaml`                  | CPU + memory autoscaler, 3–12 replicas. Latency-based scaling stub commented out (requires Prometheus Adapter). |
| `networkpolicy.yaml`        | Default-deny ingress + allow-list egress to DNS, Dynamics 365 / Entra ID (HTTPS), OTLP. **Tighten the egress CIDR to Microsoft service tags for your environment.** |
| `poddisruptionbudget.yaml`  | Guarantees ≥ 2 replicas during voluntary disruptions. |
| `kustomization.yaml`        | Kustomize entry point. Override per environment. |

## Image build

```bash
# From the repo root
docker build -t ghcr.io/gaussiantech/d365-automate:$(git rev-parse --short HEAD) -f deploy/Dockerfile .
docker push  ghcr.io/gaussiantech/d365-automate:$(git rev-parse --short HEAD)
```

The image is multi-stage; the runtime is `gcr.io/distroless/cc-debian12:nonroot`
(no shell, no package manager, no root user).

## Deploy with Kustomize

```bash
# Edit the image tag in deploy/k8s/kustomization.yaml first.
kubectl apply -k deploy/k8s/
```

## Inject a real Secret

The example file is for illustration only.  Use one of:

- **HashiCorp Vault**: register `d365-automate-secrets` as a Vault path,
  install the Vault Agent Injector, and annotate the Deployment so the
  Agent populates env vars at start-up.
- **External Secrets Operator**: declare a `SecretStore` against your
  vendor (AWS Secrets Manager / GCP Secret Manager / Azure Key Vault)
  and an `ExternalSecret` that resolves `d365-automate-secrets`.
- **Bitnami Sealed Secrets**: encrypt locally with `kubeseal`, commit
  the sealed YAML to git, the controller decrypts it in-cluster.

The Deployment loads the values via `envFrom: secretRef: ...`, so any
mechanism that creates a `Secret` named `d365-automate-secrets` works.

## Operator runbook (excerpts)

### Roll out a new image

```bash
kustomize edit set image ghcr.io/gaussiantech/d365-automate=ghcr.io/gaussiantech/d365-automate:NEW_TAG
kubectl apply -k deploy/k8s/
kubectl rollout status deploy/d365-automate-server -n d365-automate
```

The PDB guarantees ≥ 2 replicas during rollout; the `maxUnavailable: 0`
on the Deployment strategy guarantees zero downtime.

### Inspect the latency budget

```bash
kubectl port-forward -n d365-automate svc/d365-automate-server 3030:3030
curl -sS -X POST http://127.0.0.1:3030/mcp \
  -H 'authorization: Bearer $MCP_BEARER_TOKEN' \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"d365.env.health"}}' \
  | jq '.result.content[0].text | fromjson'
```

### Rotate the Entra ID client secret

1. Update the source-of-truth in your secret store.
2. The External Secrets controller (or Vault Agent) picks up the new
   value and updates the K8s `Secret`.
3. The pods read the new value on next start — restart them:

```bash
kubectl rollout restart deploy/d365-automate-server -n d365-automate
```

### Tighten the read-only-ness

The Deployment intentionally does NOT pass `--enable-writes`.  Writes
are routed through the separate `d365-automate-gw` Deployment that lives
in a dedicated namespace.  To verify the server is read-only:

```bash
kubectl logs -l app.kubernetes.io/name=d365-automate-server -n d365-automate \
  | grep "read_only=true"
```

## Multi-environment overlays

Use Kustomize overlays for dev / staging / prod:

```text
deploy/k8s/
  ├── base/      ← these manifests
  ├── overlays/
  │   ├── dev/        kustomization.yaml that patches replicas → 1, image → :dev
  │   ├── staging/    image → :rc-NN
  │   └── prod/       image → :v0.x.y
```

(Layout left to the operator — the base is intentionally environment-agnostic.)
