# Manual Ubuntu Server Setup Guide

This guide explains how to install the Dune Awakening dedicated server on a fresh Ubuntu host without using Dune Dedicated Server Manager or `dune-manager-cli`.

This path uses only:

- Ubuntu shell commands
- SteamCMD
- k3s
- the server package scripts downloaded from Steam

Use a fresh server. The setup installs packages, creates a service user, downloads server files, installs k3s, imports Kubernetes resources, writes configuration, and opens game services. Do not run it on a machine that already hosts important workloads.

## 1. Requirements

Recommended host:

- Ubuntu 24.04 or newer
- x86_64 CPU
- At least 4 CPU cores
- At least 20 GiB RAM for a basic Hagga Basin/Sietch layout
- 30 GiB RAM for Hagga Basin plus Story/Social maps
- 40 GiB RAM for Hagga Basin plus Story/Social maps plus Deep Desert
- At least 100 GiB disk
- IPv4 connectivity
- Root login or a sudo-capable user

You also need:

- A Self-Host Service Token from Funcom
- The IPv4 address players will connect to
- SSH access to the Ubuntu server

Open these firewall ports:

- TCP 22 from your own IP address for SSH
- UDP 7777-7810 from any IP for game servers
- TCP 31982 from any IP for RMQ

If you later expose the Director UI or File Browser, prefer an SSH tunnel. Do not publicly expose those services.

## 2. SSH into the server

Connect as `root`, or connect as a user that can run `sudo`.

```sh
ssh root@YOUR_SERVER_IP
```

If you are not root, keep using `sudo` where the commands below require it.

## 3. Update Ubuntu and install prerequisites

```sh
export DEBIAN_FRONTEND=noninteractive

sudo apt-get update -y
sudo apt-get install -y \
  ca-certificates curl tar gzip unzip openssl util-linux iproute2 procps \
  lsb-release sudo python3 jq lib32gcc-s1 lib32stdc++6
```

## 4. Create the `dune` service user

```sh
sudo useradd -m -s /bin/bash dune 2>/dev/null || true
sudo mkdir -p /home/dune/.dune /home/dune/.dune/bin /home/dune/.dune/download /home/dune/Steam /home/dune/.steam
sudo chown -R dune:dune /home/dune/.dune /home/dune/Steam /home/dune/.steam
```

Allow the `dune` user to run the Kubernetes and service commands used by the vendor scripts:

```sh
echo 'dune ALL=(ALL) NOPASSWD:ALL' | sudo tee /etc/sudoers.d/dune-server >/dev/null
sudo chmod 0440 /etc/sudoers.d/dune-server
```

This is convenient for a single-purpose server. If you harden it later, make sure the vendor scripts can still run their required `sudo kubectl`, `k3s`, service, and file-copy commands.

## 5. Optional: add swap on low-memory hosts

Skip this on hosts with enough RAM. On smaller hosts, swap can help the server survive memory spikes, but performance may suffer.

This example creates a 30 GiB swapfile:

```sh
sudo fallocate -l 30G /swapfile || sudo dd if=/dev/zero of=/swapfile bs=1M count=30720 status=progress
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile

grep -q '^[[:space:]]*/swapfile[[:space:]]' /etc/fstab \
  || echo '/swapfile none swap sw 0 0' | sudo tee -a /etc/fstab >/dev/null
```

If k3s will run with swap enabled, configure kubelet before installing k3s. Use a k3s config drop-in so existing k3s arguments are preserved:

```sh
sudo mkdir -p /etc/rancher/k3s/config.yaml.d
sudo tee /etc/rancher/k3s/kubelet-config.yaml >/dev/null <<'EOF'
apiVersion: kubelet.config.k8s.io/v1beta1
kind: KubeletConfiguration
imageGCHighThresholdPercent: 99
imageGCLowThresholdPercent: 98
failSwapOn: false
memorySwap:
  swapBehavior: LimitedSwap
evictionHard:
  memory.available: "100Mi"
  nodefs.available: "1%"
  nodefs.inodesFree: "1%"
  imagefs.available: "1%"
  imagefs.inodesFree: "1%"
containerLogMaxSize: "50Mi"
containerLogMaxFiles: 2
systemReserved:
  memory: "2Gi"
EOF

sudo tee /etc/rancher/k3s/config.yaml.d/99-dune-swap.yaml >/dev/null <<'EOF'
kubelet-arg+:
- config=/etc/rancher/k3s/kubelet-config.yaml
EOF
```

## 6. Install SteamCMD

```sh
tmp="$(mktemp -t steamcmd.XXXXXX.tar.gz)"
curl -fsSL 'https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz' -o "$tmp"
chmod 644 "$tmp"
sudo -u dune tar -xzf "$tmp" -C /home/dune/Steam
rm -f "$tmp"

sudo -u dune ln -sfn /home/dune/Steam /home/dune/.steam/root
sudo -u dune ln -sfn /home/dune/Steam /home/dune/.steam/steam
```

The `chmod 644 "$tmp"` line lets the `dune` user read the archive. Without it, some systems leave the temporary file readable only by the current shell user, and `tar` can fail with `Cannot open: Permission denied`.

Verify it starts:

```sh
sudo -u dune env HOME=/home/dune /home/dune/Steam/steamcmd.sh +quit
```

Add a host `steamcmd` wrapper so every call to `steamcmd` starts in a predictable environment. This avoids failures where the command runs from an arbitrary working directory and fails to resolve paths used by vendor automation.

```sh
sudo mkdir -p /home/dune/.local/bin
sudo tee /home/dune/.local/bin/steamcmd >/dev/null <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

export HOME=/home/dune
cd /home/dune/.dune/download
exec /home/dune/Steam/steamcmd.sh "$@"
EOF

sudo chown dune:dune /home/dune/.local/bin/steamcmd
sudo chmod 0755 /home/dune/.local/bin/steamcmd

sudo -u dune ln -sfn /home/dune/.local/bin/steamcmd /home/dune/.dune/bin/steamcmd
sudo ln -sfn /home/dune/.local/bin/steamcmd /usr/local/bin/steamcmd
```

If you use `steamcmd` from scripts or non-interactive shells, make sure they invoke `/home/dune/.local/bin/steamcmd` (or a symlink to it).

## 7. Download the Dune server package

The Dune dedicated server Steam app id is `4754530`.

If this host already has the old playtest package in the same download directory, clear it before installing the release package:

```sh
if [ -f /home/dune/.dune/download/steamapps/appmanifest_3104830.acf ] \
   && [ ! -f /home/dune/.dune/download/steamapps/appmanifest_4754530.acf ]; then
  sudo find /home/dune/.dune/download -mindepth 1 -maxdepth 1 -exec rm -rf {} +
  sudo mkdir -p /home/dune/.dune/download
  sudo chown -R dune:dune /home/dune/.dune/download
elif [ -f /home/dune/.dune/download/steamapps/appmanifest_3104830.acf ]; then
  sudo rm -f /home/dune/.dune/download/steamapps/appmanifest_3104830.acf
fi
```

> **Important:** SteamCMD may print `ERROR! Failed to install app '4754530' (Missing configuration)` on the first download attempt. This is usually transient. Run the exact same command below again before changing anything else.

```sh
sudo -u dune env HOME=/home/dune /home/dune/.local/bin/steamcmd \
  +@ShutdownOnFailedCommand 1 \
  +@NoPromptForPassword 1 \
  +set_spew_level 1 1 \
  +force_install_dir /home/dune/.dune/download \
  +login anonymous \
  +app_update 4754530 validate \
  +logoff \
  +quit
```

Check that the vendor scripts downloaded:

```sh
test -f /home/dune/.dune/download/scripts/setup.sh
test -f /home/dune/.dune/download/scripts/battlegroup.sh
```

If the SteamCMD command failed with `Missing configuration`, or either `test -f` command fails, rerun the SteamCMD download command above. Do this at least once before moving on.

## 8. Install k3s

```sh
curl -sfL https://get.k3s.io -o /tmp/install-k3s.sh
chmod 0755 /tmp/install-k3s.sh
sudo INSTALL_K3S_EXEC='server --disable=traefik --write-kubeconfig-mode=644' sh /tmp/install-k3s.sh
rm -f /tmp/install-k3s.sh

sudo systemctl enable k3s
sudo systemctl start k3s
```

Wait for Kubernetes to become ready:

```sh
sudo kubectl get nodes
sudo kubectl wait --for=condition=Ready node --all --timeout=180s
```

## 9. Bootstrap Kubernetes images and operators

On the bundled Hyper-V VM, the vendor scripts can rely on the VM image's prepared Kubernetes resources. On a fresh Ubuntu host, bootstrap the k3s cluster first: import the packaged images, install cert-manager, apply the Funcom CRDs, create the operator deployments, create webhook secrets and RBAC, then wait for the operators.

Create the bootstrap script:

```sh
sudo tee /root/dune-bootstrap-kubernetes.sh >/dev/null <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

DOWNLOAD_PATH=/home/dune/.dune/download

if [ ! -d "$DOWNLOAD_PATH/images/operators/crds" ]; then
  echo "Dune server payload is missing operator CRDs at $DOWNLOAD_PATH/images/operators/crds." >&2
  exit 1
fi

wait_k3s_ready() {
  local elapsed=0
  while [ ! -S /run/k3s/containerd/containerd.sock ]; do
    sleep 2
    elapsed=$((elapsed + 2))
    if [ "$elapsed" -ge 180 ]; then
      echo "k3s containerd socket did not become ready in 180s" >&2
      return 1
    fi
  done

  elapsed=0
  until sudo k3s ctr version >/dev/null 2>&1; do
    sleep 2
    elapsed=$((elapsed + 2))
    if [ "$elapsed" -ge 180 ]; then
      echo "k3s containerd did not accept commands in 180s" >&2
      return 1
    fi
  done

  elapsed=0
  until sudo kubectl get nodes >/dev/null 2>&1; do
    sleep 2
    elapsed=$((elapsed + 2))
    if [ "$elapsed" -ge 180 ]; then
      echo "k3s API did not become ready in 180s" >&2
      return 1
    fi
  done
}

load_image_from_file() {
  local file_name="$1"
  if [ ! -f "$DOWNLOAD_PATH/$file_name" ]; then
    echo "Image file $DOWNLOAD_PATH/$file_name does not exist" >&2
    exit 1
  fi

  local attempt=1
  while [ "$attempt" -le 8 ]; do
    wait_k3s_ready
    if sudo k3s ctr images import "$DOWNLOAD_PATH/$file_name"; then
      return 0
    fi
    sleep 10
    attempt=$((attempt + 1))
  done

  echo "Failed to import $file_name after 8 attempts" >&2
  exit 1
}

kubectl_retry() {
  local attempt=1
  local last_out=""
  while [ "$attempt" -le 30 ]; do
    if out=$(sudo kubectl "$@" 2>&1); then
      [ -n "$out" ] && printf '%s\n' "$out" >&2
      return 0
    fi
    last_out="$out"
    if printf '%s' "$out" | grep -qiE 'connection refused|unable to connect to the server|i/o timeout|tls handshake|no route to host|EOF|ServiceUnavailable|currently unable to handle the request|Too Many Requests|timeout awaiting response headers'; then
      sleep 10
      attempt=$((attempt + 1))
      continue
    fi
    printf '%s\n' "$out" >&2
    return 1
  done
  echo "kubectl $* still failing after retries" >&2
  [ -n "$last_out" ] && printf '%s\n' "$last_out" >&2
  return 1
}

wait_k3s_settled() {
  local elapsed=0
  local stable=0
  while [ "$elapsed" -lt 300 ]; do
    if sudo kubectl get --raw=/readyz >/dev/null 2>&1 \
      && sudo kubectl get namespaces >/dev/null 2>&1 \
      && sudo kubectl get nodes >/dev/null 2>&1; then
      stable=$((stable + 1))
      if [ "$stable" -ge 3 ]; then
        return 0
      fi
    else
      stable=0
    fi
    sleep 10
    elapsed=$((elapsed + 10))
  done
  echo "k3s API did not stay ready for 3 consecutive checks within 300s" >&2
  return 1
}

scale_deployment() {
  local ns="$1"
  local name="$2"
  local replicas="$3"
  local elapsed=0

  until sudo kubectl get -n "$ns" deployment "$name" >/dev/null 2>&1; do
    sleep 2
    elapsed=$((elapsed + 2))
    if [ "$elapsed" -ge 180 ]; then
      echo "deployment $ns/$name did not appear within 180s" >&2
      return 1
    fi
  done

  kubectl_retry scale -n "$ns" "deployment/$name" "--replicas=$replicas"
}

wait_deployment_created() {
  local ns="$1"
  local name="$2"
  local elapsed=0

  until sudo kubectl get -n "$ns" deployment "$name" >/dev/null 2>&1; do
    sleep 3
    elapsed=$((elapsed + 3))
    if [ "$elapsed" -ge 240 ]; then
      echo "deployment $ns/$name did not appear within 240s" >&2
      return 1
    fi
  done
}

wait_k3s_ready

load_image_from_file "images/prerequisites/coredns-coredns.tar"
load_image_from_file "images/prerequisites/local-path-provisioner.tar"
load_image_from_file "images/prerequisites/metrics-server.tar"
load_image_from_file "images/prerequisites/cert-manager-webhook.tar"
load_image_from_file "images/prerequisites/cert-manager-controller.tar"
load_image_from_file "images/prerequisites/cert-manager-cainjector.tar"
load_image_from_file "images/prerequisites/igw-postgres.tar"

wait_k3s_settled

if ! sudo kubectl get deployment cert-manager -n cert-manager >/dev/null 2>&1; then
  kubectl_retry apply --validate=false -f https://github.com/cert-manager/cert-manager/releases/download/v1.8.0/cert-manager.yaml
fi

wait_deployment_created cert-manager cert-manager
wait_deployment_created cert-manager cert-manager-cainjector
wait_deployment_created cert-manager cert-manager-webhook

scale_deployment kube-system coredns 1
scale_deployment kube-system local-path-provisioner 1
scale_deployment kube-system metrics-server 1
scale_deployment cert-manager cert-manager 1
scale_deployment cert-manager cert-manager-cainjector 1
scale_deployment cert-manager cert-manager-webhook 1

sudo kubectl create namespace funcom-operators --dry-run=client -o yaml | sudo kubectl apply -f -

node_name=$(sudo kubectl get nodes -o jsonpath='{.items[0].metadata.name}')
sudo kubectl label node "$node_name" node.funcom.com/workload=infrastructure --overwrite >/dev/null

load_image_from_file "images/operators/battlegroup-operator.tar"
load_image_from_file "images/operators/database-operator.tar"
load_image_from_file "images/operators/server-operator.tar"
load_image_from_file "images/operators/utilities-operator.tar"

kubectl_retry apply --server-side --validate=false -f "$DOWNLOAD_PATH/images/operators/crds/"

operator_version=$(cat "$DOWNLOAD_PATH/images/operators/version.txt")
manifest="/tmp/dune-operator-deployments.yaml"
cat > "$manifest" <<'YAML'
apiVersion: v1
kind: ServiceAccount
metadata:
  name: battlegroupoperator-controller-manager
  namespace: funcom-operators
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: databaseoperator-controller-manager
  namespace: funcom-operators
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: serveroperator-controller-manager
  namespace: funcom-operators
---
apiVersion: v1
kind: ServiceAccount
metadata:
  name: utilitiesoperator-controller-manager
  namespace: funcom-operators
---
apiVersion: apps/v1
kind: Deployment
metadata:
  labels:
    control-plane: battlegroup-controller-manager
  name: battlegroupoperator-controller-manager
  namespace: funcom-operators
spec:
  replicas: 1
  selector:
    matchLabels:
      control-plane: battlegroup-controller-manager
  template:
    metadata:
      labels:
        control-plane: battlegroup-controller-manager
    spec:
      serviceAccountName: battlegroupoperator-controller-manager
      containers:
      - name: manager
        command: ["/app/operator"]
        args:
        - --leader-elect
        - --database-default-port=15432
        - --filebrowser-default-port=18888
        - --pghero-default-port=21111
        - --zap-devel=false
        - --zap-log-level=debug
        - --zap-time-encoding=iso8601
        - --bg-max-concurrent=2
        - --dr-max-concurrent=2
        - --sr-max-concurrent=2
        - --cfo-taints-ignore=node.kubernetes.io/unschedulable,node.funcom.com/new
        image: registry.funcom.com/funcom/self-hosting/igw-k8s-battlegroup-operator:__OPERATOR_VERSION__
        imagePullPolicy: IfNotPresent
        ports:
        - containerPort: 9443
          name: webhook-server
        volumeMounts:
        - mountPath: /tmp/k8s-webhook-server/serving-certs
          name: cert
          readOnly: true
      volumes:
      - name: cert
        secret:
          secretName: battlegroupoperator-webhook-server-cert
---
apiVersion: apps/v1
kind: Deployment
metadata:
  labels:
    control-plane: database-controller-manager
  name: databaseoperator-controller-manager
  namespace: funcom-operators
spec:
  replicas: 1
  selector:
    matchLabels:
      control-plane: database-controller-manager
  template:
    metadata:
      labels:
        control-plane: database-controller-manager
    spec:
      serviceAccountName: databaseoperator-controller-manager
      containers:
      - name: manager
        command: ["/app/operator"]
        args:
        - --leader-elect
        - --zap-devel=false
        - --zap-log-level=debug
        - --zap-time-encoding=iso8601
        - --db-max-concurrent=1
        - --dbdepl-max-concurrent=1
        - --dbutil-max-concurrent=1
        - --dbop-max-concurrent=1
        - --dbb-max-concurrent=1
        - --dbbs-max-concurrent=1
        - --dbr-max-concurrent=1
        - --dbm-max-concurrent=1
        - --dbutil-supports-prometheus=false
        image: registry.funcom.com/funcom/self-hosting/igw-k8s-database-operator:__OPERATOR_VERSION__
        imagePullPolicy: IfNotPresent
        ports:
        - containerPort: 9443
          name: webhook-server
        volumeMounts:
        - mountPath: /tmp/k8s-webhook-server/serving-certs
          name: cert
          readOnly: true
      volumes:
      - name: cert
        secret:
          secretName: databaseoperator-webhook-server-cert
---
apiVersion: apps/v1
kind: Deployment
metadata:
  labels:
    control-plane: server-controller-manager
  name: serveroperator-controller-manager
  namespace: funcom-operators
spec:
  replicas: 1
  selector:
    matchLabels:
      control-plane: server-controller-manager
  template:
    metadata:
      labels:
        control-plane: server-controller-manager
    spec:
      serviceAccountName: serveroperator-controller-manager
      containers:
      - name: manager
        command: ["/app/operator"]
        args:
        - --leader-elect
        - --zap-devel=false
        - --zap-log-level=debug
        - --zap-time-encoding=iso8601
        - --sg-max-concurrent=2
        - --ss-max-concurrent=2
        image: registry.funcom.com/funcom/self-hosting/igw-k8s-server-operator:__OPERATOR_VERSION__
        imagePullPolicy: IfNotPresent
        ports:
        - containerPort: 9443
          name: webhook-server
        volumeMounts:
        - mountPath: /tmp/k8s-webhook-server/serving-certs
          name: cert
          readOnly: true
      volumes:
      - name: cert
        secret:
          secretName: serveroperator-webhook-server-cert
---
apiVersion: apps/v1
kind: Deployment
metadata:
  labels:
    control-plane: utilities-controller-manager
  name: utilitiesoperator-controller-manager
  namespace: funcom-operators
spec:
  replicas: 1
  selector:
    matchLabels:
      control-plane: utilities-controller-manager
  template:
    metadata:
      labels:
        control-plane: utilities-controller-manager
    spec:
      serviceAccountName: utilitiesoperator-controller-manager
      containers:
      - name: manager
        command: ["/app/operator"]
        args:
        - --leader-elect
        - --zap-devel=false
        - --zap-log-level=debug
        - --zap-time-encoding=iso8601
        - --sgw-max-concurrent=2
        - --bgd-max-concurrent=2
        - --fb-max-concurrent=1
        - --mq-max-concurrent=2
        - --tr-max-concurrent=2
        image: registry.funcom.com/funcom/self-hosting/igw-k8s-utilities-operator:__OPERATOR_VERSION__
        imagePullPolicy: IfNotPresent
        ports:
        - containerPort: 9443
          name: webhook-server
        volumeMounts:
        - mountPath: /tmp/k8s-webhook-server/serving-certs
          name: cert
          readOnly: true
      volumes:
      - name: cert
        secret:
          secretName: utilitiesoperator-webhook-server-cert
YAML

sed -i "s/__OPERATOR_VERSION__/$operator_version/g" "$manifest"
kubectl_retry apply --validate=false -f "$manifest"
rm -f "$manifest"

for op in battlegroupoperator databaseoperator serveroperator utilitiesoperator; do
  secret="${op}-webhook-server-cert"
  if ! sudo kubectl get secret "$secret" -n funcom-operators >/dev/null 2>&1; then
    sudo openssl req -x509 -nodes -newkey rsa:2048 -days 3650 \
      -keyout /tmp/dune-webhook.key -out /tmp/dune-webhook.crt \
      -subj "/CN=${op}-webhook.funcom-operators.svc" >/dev/null 2>&1
    sudo kubectl create secret tls "$secret" -n funcom-operators \
      --cert=/tmp/dune-webhook.crt --key=/tmp/dune-webhook.key >/dev/null
    sudo rm -f /tmp/dune-webhook.key /tmp/dune-webhook.crt
  fi

  if ! sudo kubectl get clusterrolebinding "${op}-manager-rolebinding" >/dev/null 2>&1; then
    sudo kubectl create clusterrolebinding "${op}-manager-rolebinding" \
      --clusterrole="${op}-manager-role" \
      --serviceaccount="funcom-operators:${op}-controller-manager" >/dev/null
  fi

  if ! sudo kubectl get role "${op}-leader-election-role" -n funcom-operators >/dev/null 2>&1; then
    sudo kubectl create role "${op}-leader-election-role" \
      -n funcom-operators \
      --verb=get,list,watch,create,update,patch,delete \
      --resource=leases.coordination.k8s.io \
      --resource=events >/dev/null
    sudo kubectl create rolebinding "${op}-leader-election-rolebinding" \
      -n funcom-operators \
      --role="${op}-leader-election-role" \
      --serviceaccount="funcom-operators:${op}-controller-manager" >/dev/null
  fi
done

scale_deployment funcom-operators battlegroupoperator-controller-manager 1
scale_deployment funcom-operators databaseoperator-controller-manager 1
scale_deployment funcom-operators serveroperator-controller-manager 1
scale_deployment funcom-operators utilitiesoperator-controller-manager 1

sudo kubectl wait --for=condition=Available -n funcom-operators deployment/battlegroupoperator-controller-manager --timeout=180s
sudo kubectl wait --for=condition=Available -n funcom-operators deployment/databaseoperator-controller-manager --timeout=180s
sudo kubectl wait --for=condition=Available -n funcom-operators deployment/serveroperator-controller-manager --timeout=180s
sudo kubectl wait --for=condition=Available -n funcom-operators deployment/utilitiesoperator-controller-manager --timeout=180s
EOF

sudo chmod +x /root/dune-bootstrap-kubernetes.sh
```

Run it:

```sh
sudo /root/dune-bootstrap-kubernetes.sh
```

## 10. Write the player-facing IP

Pick the IP address players should use. For a public VPS, this is usually the server's public IPv4 address.

```sh
PLAYER_IP="YOUR_PUBLIC_OR_PLAYER_FACING_IP"

printf '\n\n\n%s\n' "$PLAYER_IP" | sudo tee /home/dune/.dune/settings.conf >/dev/null
sudo chown dune:dune /home/dune/.dune/settings.conf
```

## 11. Patch the vendor scripts for Ubuntu

The downloaded vendor scripts currently assume Alpine/OpenRC for a few service commands. Ubuntu uses systemd, so patch those commands before running `setup.sh`.

Run this from your root shell:

```sh
cd /home/dune/.dune/download/scripts

for file in setup/k3s.sh setup/helper.sh setup/experimental_swap.sh; do
  if [ -f "$file" ] && [ ! -f "$file.ubuntu-patch.bak" ]; then
    cp -p "$file" "$file.ubuntu-patch.bak"
  fi
done

python3 - <<'PY'
from pathlib import Path

replacements = {
    "setup/k3s.sh": {
        "sudo rc-service k3s start": "sudo systemctl start k3s",
        "sudo rc-update add k3s": "sudo systemctl enable k3s",
    },
    "setup/helper.sh": {
        "sudo rc-service k3s restart": "sudo systemctl restart k3s",
    },
    "setup/experimental_swap.sh": {
        "sudo rc-update add swap boot": "sudo systemctl daemon-reload || true",
        "sudo rc-service k3s stop": "sudo systemctl stop k3s",
        "sudo rc-service k3s restart": "sudo systemctl restart k3s",
        "echo 0 | sudo tee /sys/fs/cgroup/openrc.k3s/memory.swap.max": (
            "if [ -w /sys/fs/cgroup/system.slice/k3s.service/memory.swap.max ]; "
            "then echo 0 | sudo tee /sys/fs/cgroup/system.slice/k3s.service/memory.swap.max >/dev/null; fi"
        ),
    },
}

for relative_path, file_replacements in replacements.items():
    path = Path(relative_path)
    text = path.read_text()
    for old, new in file_replacements.items():
        text = text.replace(old, new)
    path.write_text(text)
PY

chmod +x setup.sh setup/k3s.sh setup/helper.sh setup/experimental_swap.sh battlegroup.sh

if grep -RIn --exclude='*.ubuntu-patch.bak' 'rc-service\|rc-update\|openrc.k3s' setup; then
  echo "Some Alpine/OpenRC references remain (listed above). Read the note below before running setup.sh."
else
  echo "Vendor scripts are patched for Ubuntu."
fi
```

This check runs in your login shell, so it only prints a message. It does not stop anything or close your SSH session.

Newer server packages can add scripts that this patch does not cover yet, such as `setup/vm_ip.sh`. If the check lists lines from such a file, look at each one before running `setup.sh`. Ubuntu has no `rc-service` or `rc-update`, so the matching systemd commands are:

- `rc-service NAME start|stop|restart` becomes `systemctl start|stop|restart NAME`
- `rc-update add NAME` becomes `systemctl enable NAME`

Change a line only when `setup.sh` actually runs it on your host. Lines that apply only to the Alpine-based Hyper-V VM can stay as they are.

## 12. Run the vendor setup script

The fresh k3s cluster is now prepared for the Funcom operators. Run the downloaded vendor setup script as the `dune` user to create the world resources:

```sh
sudo mkdir -p /home/dune/.dune/bin /home/dune/.dune/download
sudo chown -R dune:dune /home/dune/.dune

sudo -iu dune
cd /home/dune/.dune/download
chmod +x scripts/setup.sh scripts/battlegroup.sh
./scripts/setup.sh
```

The `/home/dune/.dune/bin` directory must exist before `setup.sh` reaches its helper-link step. If it is missing, setup can print errors like `ln: failed to create symbolic link '/home/dune/.dune/bin/battlegroup': No such file or directory`.

Follow the prompts from the vendor script. When asked, provide:

- Your Self-Host Service Token
- The world/server name you want players to see
- The region: `Asia`, `Europe`, `North America`, `Oceania`, or `South America`
- Any layout or map choices offered by the script

Do not paste the Self-Host Service Token into shared logs, screenshots, chat, or issue reports.

After setup finishes, leave the `dune` shell:

```sh
exit
```

Back in your root shell, patch the created BattleGroup so the gateway declares your player-facing IP instead of the vendor template's default loopback address. This value is surfaced through Funcom server-list metadata and is separate from ordinary ICMP ping.

```sh
PLAYER_IP="YOUR_PUBLIC_OR_PLAYER_FACING_IP"
NS="$(sudo kubectl get ns --no-headers -o custom-columns=NAME:.metadata.name | grep '^funcom-seabass-' | head -n1)"
BG="${NS#funcom-seabass-}"

if [ -f "/home/dune/.dune/$BG.yaml" ]; then
  sudo python3 - "/home/dune/.dune/$BG.yaml" "$PLAYER_IP" <<'PY'
import sys

path, player_ip = sys.argv[1], sys.argv[2]
lines = open(path, encoding="utf-8").read().splitlines(keepends=True)
next_is_host_ip = False
replaced = 0

for index, line in enumerate(lines):
    if "name: HOST_DATACENTER_IP_ADDRESS" in line:
        next_is_host_ip = True
        continue
    if next_is_host_ip:
        if "value:" in line:
            indent = line[: len(line) - len(line.lstrip())]
            newline = "\n" if line.endswith("\n") else ""
            lines[index] = f"{indent}value: {player_ip}{newline}"
            replaced += 1
        next_is_host_ip = False

if replaced == 0:
    raise SystemExit("No HOST_DATACENTER_IP_ADDRESS values were found in the generated world manifest")

open(path, "w", encoding="utf-8").writelines(lines)
PY
fi

PATCH="$(
  sudo kubectl get battlegroup "$BG" -n "$NS" -o json \
    | PLAYER_IP="$PLAYER_IP" python3 -c 'import json,os,sys
bg=json.load(sys.stdin)
player_ip=os.environ["PLAYER_IP"]
ops=[]
def esc(part):
    return str(part).replace("~","~0").replace("/","~1")
def walk(node,path):
    if isinstance(node,dict):
        envs=node.get("envVars")
        if isinstance(envs,list):
            for i,item in enumerate(envs):
                if isinstance(item,dict) and item.get("name")=="HOST_DATACENTER_IP_ADDRESS":
                    ops.append({"op":"replace" if "value" in item else "add","path":"/"+"/".join(esc(p) for p in path+["envVars",i,"value"]),"value":player_ip})
        for key,value in node.items():
            walk(value,path+[key])
    elif isinstance(node,list):
        for i,value in enumerate(node):
            walk(value,path+[i])
walk(bg.get("spec",{}),["spec"])
print(json.dumps(ops))'
)"

if [ "$PATCH" != "[]" ]; then
  sudo kubectl patch battlegroup "$BG" -n "$NS" --type=json -p "$PATCH"
fi
```

Remove any custom scheduler references from the created BattleGroup. Fresh k3s hosts should use the default Kubernetes scheduler.

```sh
PATCH="$(
  sudo kubectl get battlegroup "$BG" -n "$NS" -o json \
    | python3 -c 'import json,sys
bg=json.load(sys.stdin)
sets=bg.get("spec",{}).get("serverGroup",{}).get("template",{}).get("spec",{}).get("sets",[])
ops=[{"op":"remove","path":f"/spec/serverGroup/template/spec/sets/{i}/schedulerName"} for i,item in enumerate(sets) if "schedulerName" in item]
print(json.dumps(ops))'
)"

if [ "$PATCH" != "[]" ]; then
  sudo kubectl patch battlegroup "$BG" -n "$NS" --type=json -p "$PATCH"
fi
```

Align the generated PostgreSQL credentials with the running database pod before schema initialization retries. On fresh Ubuntu clusters, the database container may start with only the `postgres` role/database, while the Dune schema utility connects as the generated game user. This step creates or updates that game role and database from the live `DatabaseDeployment` values.

Do not paste command output or full Kubernetes object YAML into shared logs; they contain database credentials.

```sh
DDEP="$BG-db-dbdepl"
if ! sudo kubectl get databasedeployment "$DDEP" -n "$NS" >/dev/null 2>&1; then
  DDEP="$(
    sudo kubectl get databasedeployments -n "$NS" --no-headers -o custom-columns=NAME:.metadata.name \
      | awk -v bg="$BG" '$1 ~ "^" bg ".*dbdepl$" { print $1; exit }'
  )"
fi

DBPOD="$DDEP-sts-0"
elapsed=0
while [ "$elapsed" -lt 180 ]; do
  phase="$(sudo kubectl get pod "$DBPOD" -n "$NS" -o jsonpath='{.status.phase}' 2>/dev/null || true)"
  [ "$phase" = "Running" ] && break
  sleep 5
  elapsed=$((elapsed + 5))
done

if [ "${phase:-}" = "Running" ]; then
  echo "Database pod $DBPOD is running. Continue with the next block."
else
  echo "Database pod $DBPOD is not running yet. Wait a minute and rerun this block before continuing."
fi
```

Continue only after the block above prints that the database pod is running:

```sh
SUPER_USER="$(sudo kubectl get databasedeployment "$DDEP" -n "$NS" -o jsonpath='{.spec.superUser}')"
SUPER_PASSWORD="$(sudo kubectl get databasedeployment "$DDEP" -n "$NS" -o jsonpath='{.spec.superPassword}')"
DB_PORT="$(sudo kubectl get databasedeployment "$DDEP" -n "$NS" -o jsonpath='{.spec.port}')"
DB_USER="$(sudo kubectl get databasedeployment "$DDEP" -n "$NS" -o jsonpath='{.spec.user}')"
DB_PASSWORD="$(sudo kubectl get databasedeployment "$DDEP" -n "$NS" -o jsonpath='{.spec.password}')"
DB_NAME="$(sudo kubectl get databasedeployment "$DDEP" -n "$NS" -o jsonpath='{.spec.gameDatabaseName}')"

[ -n "$SUPER_USER" ] || SUPER_USER=postgres
[ -n "$DB_PORT" ] || DB_PORT=15432
[ -n "$DB_USER" ] || DB_USER=dune
[ -n "$DB_NAME" ] || DB_NAME=dune

sudo kubectl exec -i -n "$NS" "$DBPOD" -- \
  psql -h 127.0.0.1 -p "$DB_PORT" -U "$SUPER_USER" -d postgres \
    -v ON_ERROR_STOP=1 \
    -v db_user="$DB_USER" \
    -v db_password="$DB_PASSWORD" \
    -v db_name="$DB_NAME" >/dev/null <<'SQL'
SELECT format('CREATE ROLE %I LOGIN PASSWORD %L', :'db_user', :'db_password')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = :'db_user') \gexec
ALTER ROLE :"db_user" WITH LOGIN PASSWORD :'db_password';
SELECT format('CREATE DATABASE %I OWNER %I', :'db_name', :'db_user')
WHERE NOT EXISTS (SELECT 1 FROM pg_database WHERE datname = :'db_name') \gexec
SQL

sudo kubectl exec -i -n "$NS" "$DBPOD" -- \
  psql -h 127.0.0.1 -p "$DB_PORT" -U "$SUPER_USER" -d postgres \
    -v ON_ERROR_STOP=1 \
    -v super_user="$SUPER_USER" \
    -v super_password="$SUPER_PASSWORD" >/dev/null <<'SQL'
ALTER ROLE :"super_user" WITH PASSWORD :'super_password';
SQL
```

If schema initialization already failed once, the database operator should retry. Watch it with:

```sh
sudo kubectl get databasedeployments -n "$NS" -w
```

## 13. Install the battlegroup helper shortcuts

The vendor setup normally creates these helpers. Run these commands anyway if `/home/dune/.dune/bin/battlegroup` or `/home/dune/.dune/bin/bg-util` is missing or not executable.

```sh
sudo mkdir -p /home/dune/.dune/bin
sudo ln -sfn /home/dune/.dune/download/scripts/battlegroup.sh /home/dune/.dune/bin/battlegroup
sudo chmod +x /home/dune/.dune/download/scripts/battlegroup.sh
sudo chown -h dune:dune /home/dune/.dune/bin/battlegroup

sudo ln -sfn /home/dune/.dune/download/scripts/bg-util /home/dune/.dune/bin/bg-util
sudo chmod +x /home/dune/.dune/download/scripts/bg-util
sudo chown -h dune:dune /home/dune/.dune/bin/bg-util
```

You can now manage the server from SSH with:

```sh
sudo -iu dune
/home/dune/.dune/bin/battlegroup status
/home/dune/.dune/bin/battlegroup start
```

## 14. Verify Kubernetes resources

List battlegroup namespaces:

```sh
sudo kubectl get ns | grep '^funcom-seabass-'
```

Set variables for the namespace and battlegroup name:

```sh
NS="$(sudo kubectl get ns --no-headers -o custom-columns=NAME:.metadata.name | grep '^funcom-seabass-' | head -n1)"
BG="${NS#funcom-seabass-}"

echo "Namespace: $NS"
echo "BattleGroup: $BG"
```

Check the BattleGroup:

```sh
sudo kubectl get battlegroup "$BG" -n "$NS" -o wide
sudo kubectl get pods -n "$NS" -o wide
```

Start it if it is stopped:

```sh
sudo kubectl patch battlegroup "$BG" -n "$NS" --type=merge -p '{"spec":{"stop":false}}'
```

It can take several minutes before all pods are running and the server appears in-game.

## 15. Useful management commands

Status:

```sh
sudo -iu dune /home/dune/.dune/bin/battlegroup status
```

Start:

```sh
sudo -iu dune /home/dune/.dune/bin/battlegroup start
```

Stop:

```sh
sudo -iu dune /home/dune/.dune/bin/battlegroup stop
```

Restart:

```sh
sudo -iu dune /home/dune/.dune/bin/battlegroup restart
```

Update from Steam:

```sh
sudo -iu dune /home/dune/.dune/bin/battlegroup update
```

After an update, check that no pods are stuck in `Pending`. See "Pods stay Pending after a server update" under Troubleshooting.

Back up the game database:

```sh
sudo -iu dune /home/dune/.dune/bin/battlegroup backup my-backup-name
```

This dumps only the PostgreSQL game database. It does not back up Kubernetes secrets, RabbitMQ, logs, or the rest of the server. Current server packages write the dump to `Saved/DatabaseDumps/<name>` on the game server's persistent volume, which is under `/var/lib/rancher/k3s/storage` with k3s local-path storage. Older packages wrote to `/funcom/artifacts/database-dumps/<battlegroup>/` instead. To find existing dumps:

```sh
sudo find /var/lib/rancher/k3s/storage /funcom/artifacts/database-dumps \
  -name '*.backup' 2>/dev/null
```

Restoring a backup with `battlegroup import` overwrites the game database. Stop the BattleGroup first.

Export logs:

```sh
sudo -iu dune /home/dune/.dune/bin/battlegroup logs-export
sudo -iu dune /home/dune/.dune/bin/battlegroup operator-logs-export
```

Raw Kubernetes checks:

```sh
sudo kubectl get pods -A
sudo kubectl get battlegroups -A
sudo kubectl describe battlegroup "$BG" -n "$NS"
```

## 16. Access Director or File Browser safely

Do not expose Director or File Browser directly to the public internet.

For Director, first discover the NodePort:

```sh
sudo kubectl get svc -A -o jsonpath='{.items[*].spec.ports[?(@.port==11717)].nodePort}'
echo
```

From your local machine, create an SSH tunnel. Replace `DIRECTOR_NODEPORT` with the value above:

```sh
ssh -L 11717:YOUR_SERVER_IP:DIRECTOR_NODEPORT root@YOUR_SERVER_IP
```

Then open:

```text
http://127.0.0.1:11717/
```

For File Browser, the vendor management script opens TCP `18888` on the server. Tunnel it instead of opening it publicly:

```sh
ssh -L 18888:127.0.0.1:18888 root@YOUR_SERVER_IP
```

Then open:

```text
http://127.0.0.1:18888/
```

## 17. Troubleshooting

SteamCMD download fails:

```sh
sudo -u dune env HOME=/home/dune /home/dune/Steam/steamcmd.sh +quit
```

Then rerun the `app_update 4754530 validate` command.

k3s is not ready:

```sh
sudo systemctl status k3s --no-pager
sudo journalctl -u k3s -n 200 --no-pager
sudo kubectl get nodes -o wide
```

Pods are stuck:

```sh
sudo kubectl get pods -A
sudo kubectl describe pod -n "$NS" POD_NAME
sudo kubectl logs -n "$NS" POD_NAME --all-containers --tail=200
```

Pods stay Pending after a server update:

Server updates can add new map sets that pin pods to a custom `memory-focused-scheduler`. Fresh k3s hosts do not run that scheduler, so the new pods stay `Pending` with no node and no scheduling events, and players can get stuck in the travel queue. Check for scheduler references:

```sh
sudo kubectl get pods -n "$NS" --field-selector=status.phase=Pending \
  -o custom-columns='NAME:.metadata.name,SCHEDULER:.spec.schedulerName,NODE:.spec.nodeName'
```

If any pod shows `memory-focused-scheduler`, rerun the "Remove any custom scheduler references" step from section 12. It re-reads the current set indices and removes the field from every set. The operator then moves the pending pods to the default scheduler without a full BattleGroup restart.

Operators are not ready:

```sh
sudo kubectl get pods -n funcom-operators
sudo kubectl logs -n funcom-operators deployment/battlegroupoperator-controller-manager --all-containers --tail=200
sudo kubectl logs -n funcom-operators deployment/databaseoperator-controller-manager --all-containers --tail=200
sudo kubectl logs -n funcom-operators deployment/serveroperator-controller-manager --all-containers --tail=200
sudo kubectl logs -n funcom-operators deployment/utilitiesoperator-controller-manager --all-containers --tail=200
```

The server does not appear in-game:

- Confirm UDP `7777-7810` and TCP `31982` are open in the host firewall and provider firewall.
- Confirm `PLAYER_IP` in `/home/dune/.dune/settings.conf` is the address players can actually reach.
- Confirm the BattleGroup is not stopped:

```sh
sudo kubectl get battlegroup "$BG" -n "$NS" -o jsonpath='{.spec.stop}{"\n"}'
```

If it prints `true`, start it:

```sh
sudo kubectl patch battlegroup "$BG" -n "$NS" --type=merge -p '{"spec":{"stop":false}}'
```
