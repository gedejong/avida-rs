#!/bin/bash
# Run a lab batch on a Hetzner Cloud server made for it and deleted afterwards.
# Needs curl, jq, ssh, rsync and a token of a Hetzner project used for nothing
# else, in the environment as AVIDA_HETZNER_API_KEY or in ~/.config/avida-cloud/env
# (mode 600; never on a command line, never in a repository).
#
#   tools/cloud/cloud.sh up                  make the server, build avida on it
#   tools/cloud/cloud.sh run LABDIR [ENV..]  copy a lab folder over and start its run_all.sh
#                                            (e.g. run ~/avida-lab/15-xor-wall JOBS=16)
#   tools/cloud/cloud.sh status              server age and cost, batch log, load
#   tools/cloud/cloud.sh fetch LABDIR        copy the runs back into the lab folder
#   tools/cloud/cloud.sh ssh [command]       a shell on the server
#   tools/cloud/cloud.sh down --yes [all]    delete the server (billing stops), firewall and key stay
#   tools/cloud/cloud.sh purge --yes         delete everything this script made in the project
#
# Several cheap servers as one pool (cx and cax types only, 32 cores in all by default):
#   tools/cloud/cloud.sh fleet-up            make servers up to AVIDA_CLOUD_CORES, build on each
#   tools/cloud/cloud.sh fleet-run LABDIR [ENV..]   deal the lab's runs over the servers and start them
#                                            (AVIDA_CLOUD_LOCAL=6 keeps six cores' share for this machine)
#   tools/cloud/cloud.sh fleet-status        every server: cost so far, progress
#   tools/cloud/cloud.sh fleet-fetch LABDIR  copy all finished runs into the lab folder
#   tools/cloud/cloud.sh down --yes all      delete every server
#
# A server is billed per started hour for as long as it exists, running or
# not: `down` when the results are fetched. Results on another architecture or
# C library are not bit-identical with local runs (the port pins values on
# macOS arm64); compare arms that ran on the same machine.
set -euo pipefail

PROJECT=avida-lab
TYPES=${AVIDA_CLOUD_TYPES:-"cax41 cx53 cpx62"}
LOCATIONS=${AVIDA_CLOUD_LOCATIONS:-"fsn1 nbg1 hel1"}
KEY=${AVIDA_CLOUD_KEY:-$HOME/.ssh/avida-cloud}
# More than one server: AVIDA_CLOUD_SERVER=b tools/cloud/cloud.sh up (and the same for
# run, status, fetch, ssh, down). Each name is a server of its own.
set_role() { ROLE=$1; if [ "$ROLE" = job ]; then KNOWN=$KEY.known_hosts; ALIAS=avida-cloud; else KNOWN=$KEY.$ROLE.known_hosts; ALIAS=avida-cloud-$ROLE; fi; }
set_role "${AVIDA_CLOUD_SERVER:-job}"
# The fleet: only the cheap shared families, largest first, up to FLEET_CORES in all.
CHEAP=${AVIDA_CLOUD_CHEAP:-"cx53 cax41 cx43 cax31 cx33 cax21"}
FLEET_CORES=${AVIDA_CLOUD_CORES:-32}
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
API=https://api.hetzner.cloud/v1
SEL="label_selector=project=$PROJECT"

die() { echo "cloud: $*" >&2; exit 1; }

# The token: from the environment, or from a private file outside any repository.
if [ -z "${AVIDA_HETZNER_API_KEY:-}" ] && [ -f "$HOME/.config/avida-cloud/env" ]; then
  AVIDA_HETZNER_API_KEY=$(sed -n 's/^AVIDA_HETZNER_API_KEY=//p' "$HOME/.config/avida-cloud/env")
fi

hz() { # hz METHOD path [json]
  local method=$1 path=$2 data=${3:-} out status
  [ -n "${AVIDA_HETZNER_API_KEY:-}" ] || die "no AVIDA_HETZNER_API_KEY in the environment"
  out=$(curl -sS --max-time 60 -X "$method" \
    -H @<(printf 'Authorization: Bearer %s\n' "$AVIDA_HETZNER_API_KEY") \
    -H 'Content-Type: application/json' ${data:+--data "$data"} \
    -w '\n%{http_code}' "$API$path") || { echo "cloud: $method $path: no answer" >&2; return 1; }
  status=${out##*$'\n'}; out=${out%$'\n'*}
  if [ "${status:0:1}" != 2 ]; then
    echo "cloud: $method $path: HTTP $status $(jq -r '.error | "\(.code): \(.message)"' <<< "$out" 2>/dev/null)" >&2
    return 1
  fi
  printf '%s\n' "$out"
}

# The token must belong to a project that holds nothing but this script's own
# things: another project's core limit and teardown-by-label are not ours to touch.
own_project() {
  local foreign
  foreign=$( { hz GET "/servers?per_page=50" | jq -r '.servers[] | select(.labels.project != "'$PROJECT'") | "server \(.name)"'
               hz GET "/images?type=snapshot&per_page=50" | jq -r '.images[] | select(.labels.project != "'$PROJECT'") | "snapshot \(.description)"'; } )
  [ -z "$foreign" ] || die "the token's project holds things of another service; use a project of its own:"$'\n'"$foreign"
}

server() { hz GET "/servers?$SEL,role=$ROLE&sort=created:desc" | jq -c '.servers[0] // empty'; }
ip() { server | jq -r '.public_net.ipv4.ip // empty'; }
wait_action() { # id
  local s
  while :; do
    s=$(hz GET "/actions/$1" | jq -r .action.status)
    [ "$s" = success ] && return 0
    [ "$s" = error ] && return 1
    sleep 3
  done
}
remote() { # [command..]
  local addr; addr=$(ip); [ -n "$addr" ] || die "no server; run: cloud.sh up"
  ssh -i "$KEY" -o BatchMode=yes -o ConnectTimeout=5 -o ServerAliveInterval=15 -o ServerAliveCountMax=8 \
      -o HostKeyAlias=$ALIAS -o UserKnownHostsFile="$KNOWN" -o StrictHostKeyChecking=accept-new \
      "root@$addr" "$@"
}
rsync_to() { # rsync args.., with the remote written as HOST:path
  local addr; addr=$(ip); [ -n "$addr" ] || die "no server"
  local args=(); for a in "$@"; do args+=("${a/HOST:/root@$addr:}"); done
  rsync -e "ssh -i $KEY -o BatchMode=yes -o HostKeyAlias=$ALIAS -o UserKnownHostsFile=$KNOWN -o StrictHostKeyChecking=accept-new" "${args[@]}"
}

firewall() { # the firewall's id, with port 22 open to this machine's address only
  local me id rules
  me=$(curl -sS --max-time 20 https://api.ipify.org) || die "cannot read this machine's public address"
  [[ $me =~ ^[0-9.]+$ ]] || die "unexpected public address: $me"
  rules=$(jq -nc --arg ip "$me/32" '{rules: [{direction: "in", protocol: "tcp", port: "22", source_ips: [$ip]}]}')
  id=$(hz GET "/firewalls?$SEL" | jq -r '.firewalls[0].id // empty')
  if [ -z "$id" ]; then
    id=$(hz POST /firewalls "$(jq -nc --arg n "$PROJECT" '{name: $n, labels: {project: $n}, rules: []}')" | jq -r .firewall.id)
  fi
  hz POST "/firewalls/$id/actions/set_rules" "$rules" > /dev/null
  echo "$id"
}

ssh_key() { # the registered key's id
  local id
  [ -f "$KEY" ] || ssh-keygen -q -t ed25519 -N '' -C "$PROJECT" -f "$KEY"
  chmod 600 "$KEY"
  id=$(hz GET "/ssh_keys?$SEL" | jq -r '.ssh_keys[0].id // empty')
  if [ -z "$id" ]; then
    id=$(hz POST /ssh_keys "$(jq -nc --arg n "$PROJECT" --arg k "$(cat "$KEY.pub")" '{name: $n, public_key: $k, labels: {project: $n}}')" | jq -r .ssh_key.id)
  fi
  echo "$id"
}

make_server() { # TYPES..: make a server for $ROLE from the first type and location to be had; 75 when none
  local fw key body out type loc
  fw=$(firewall); key=$(ssh_key)
  for type in "$@"; do for loc in $LOCATIONS; do
    body=$(jq -nc --arg name "$PROJECT-$ROLE-$(date +%Y%m%d-%H%M%S)" --arg role "$ROLE" --arg type "$type" --arg loc "$loc" \
      --argjson key "$key" --argjson fw "$fw" --arg p "$PROJECT" \
      '{name: $name, server_type: $type, location: $loc, image: "ubuntu-24.04", ssh_keys: [$key],
        firewalls: [{firewall: $fw}], labels: {project: $p, role: $role},
        public_net: {enable_ipv4: true, enable_ipv6: false}, start_after_create: true}')
    if out=$(hz POST /servers "$body" 2> "$KEY.err"); then
      if wait_action "$(jq -r .action.id <<< "$out")"; then
        echo "cloud: made $type in $loc as $ROLE"; rm -f "$KNOWN" "$KEY.err"; return 0
      fi
      # Accepted and then failed, as when there is no room after all: remove what is left of it.
      hz DELETE "/servers/$(jq -r .server.id <<< "$out")" > /dev/null 2>&1 || true
      echo "cloud: no $type in $loc (the making failed), trying the next"
    elif grep -Eq 'HTTP (412|503)|resource_unavailable|placement_error|server_type_unavailable|unsupported location' "$KEY.err"; then
      echo "cloud: no $type in $loc ($(sed -n 's/.*HTTP [0-9]* \([a-z_]*\):.*/\1/p' "$KEY.err")), trying the next"
    else cat "$KEY.err" >&2; die "making the server was refused"
    fi
  done; done
  return 75
}

ready() { # wait for ssh, then build
  echo "cloud: $ROLE: waiting for ssh"
  for _ in $(seq 60); do remote true 2> /dev/null && break; sleep 5; done
  remote true || die "$ROLE does not answer ssh"
  build
}

up() {
  own_project
  if [ -n "$(server)" ]; then echo "cloud: server exists: $(server | jq -r '"\(.name) \(.server_type.name) \(.public_net.ipv4.ip)"')"
  else
    # shellcheck disable=SC2086
    make_server $TYPES || { echo "cloud: no server to be had ($TYPES in $LOCATIONS)" >&2; exit 75; }
  fi
  ready
}

build() {
  echo "cloud: copying the source and building (some minutes the first time)"
  remote 'mkdir -p /root/avida/rust /root/avida/avida-core /root/lab/bin'
  rsync_to -a --delete --exclude target --exclude 'target_*' "$REPO/rust/avida-native" HOST:/root/avida/rust/
  rsync_to -a --delete "$REPO/avida-core/support" HOST:/root/avida/avida-core/
  remote 'set -e
    export DEBIAN_FRONTEND=noninteractive
    command -v cc > /dev/null || { apt-get update -q > /dev/null && apt-get install -y -q build-essential rsync > /dev/null; }
    [ -x /root/.cargo/bin/cargo ] || curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none > /dev/null
    cd /root/avida/rust/avida-native && /root/.cargo/bin/cargo build --release -p avida 2>&1 | tail -2
    cp target/release/avida /root/lab/bin/avida && /root/lab/bin/avida --version 2> /dev/null | head -1 || true
    nproc'
}

run() { # LABDIR [ENV=value..]
  local lab=${1:?lab folder}; shift
  local name; name=$(basename "$lab")
  [ -x "$lab/run_all.sh" ] || die "$lab has no run_all.sh"
  remote "pgrep -x avida > /dev/null" && die "a batch is already running on the server"
  rsync_to -a --exclude 'runs*' --exclude '*.log' --exclude recordings "$lab/" "HOST:/root/lab/$name/"
  # shellcheck disable=SC2029
  remote "cd /root/lab/$name && env $* setsid -f nohup ./run_all.sh > batch.log 2>&1 < /dev/null; sleep 1; echo started in /root/lab/$name"
}

status() {
  local s; s=$(server); [ -n "$s" ] || { echo "cloud: no server"; return; }
  jq -r '"\(.name) \(.server_type.name) \(.datacenter.location.name // .location.name) \(.status) created \(.created)"' <<< "$s"
  local type loc created hours price
  type=$(jq -r .server_type.name <<< "$s"); loc=$(jq -r '.datacenter.location.name // .location.name' <<< "$s"); created=$(jq -r .created <<< "$s")
  hours=$(python3 -c "import sys,datetime,math; t=datetime.datetime.fromisoformat(sys.argv[1]); print(math.ceil((datetime.datetime.now(datetime.timezone.utc)-t).total_seconds()/3600))" "$created")
  price=$(jq -r --arg l "$loc" '.server_type.prices[] | select(.location == $l) | .price_hourly.gross' <<< "$s")
  echo "started hours: $hours, about EUR $(python3 -c "print(round($hours*$price,2))") so far"
  remote 'uptime; for d in /root/lab/*/; do [ -f "$d/batch.log" ] || continue; echo "== $d: $(find "$d" -name .done | wc -l) runs done, $(pgrep -xc avida || true) running"; tail -3 "$d/batch.log"; done'
}

fetch() { # LABDIR
  local lab=${1:?lab folder}; local name; name=$(basename "$lab")
  # The per-update resource maps (resource_*.m) are most of a run's bytes and no
  # analysis reads them: left on the server unless AVIDA_CLOUD_MAPS=1.
  local maps=(--exclude 'resource_*.m'); [ "${AVIDA_CLOUD_MAPS:-}" = 1 ] && maps=()
  rsync_to -az "${maps[@]}" --include 'runs*/***' --include 'batch.log' --exclude '*' "HOST:/root/lab/$name/" "$lab/cloud/"
  echo "cloud: results are in $lab/cloud/"
}

# ---- the fleet: several cheap servers as one pool --------------------------------
# A lab's run_all.sh must answer LIST=1 with its runs, one "arm seed" a line, and
# run only the lines of PAIRS_FILE when that is set.

fleet() { hz GET "/servers?$SEL&per_page=50" | jq -r '.servers[] | "\(.labels.role) \(.server_type.cores) \(.server_type.name)"' | sort; }

fleet_up() {
  own_project
  local have want type cores n=0 role types
  types=$(hz GET "/server_types?per_page=50" | jq -r '.server_types[] | "\(.name) \(.cores)"')
  have=$(fleet | awk '{s += $2} END {print s + 0}'); want=$FLEET_CORES
  for type in $CHEAP; do
    cores=$(awk -v t="$type" '$1 == t {print $2}' <<< "$types"); [ -n "$cores" ] || continue
    while [ $((have + cores)) -le "$want" ]; do
      n=$((n + 1)); role=f$n; fleet | grep -q "^$role " && continue
      set_role "$role"
      if make_server "$type"; then have=$((have + cores)); else break; fi
    done
  done
  echo "cloud: the fleet has $have of $want cores"
  for role in $(fleet | cut -d' ' -f1); do ( set_role "$role"; ready > "$KEY.$role.build.log" 2>&1 || echo "cloud: $role: build failed, see $KEY.$role.build.log" ) & done
  wait
  fleet_status
}

fleet_run() { # LABDIR [ENV=value..]   AVIDA_CLOUD_LOCAL=n gives n cores' share to this machine
  local lab=${1:?lab folder}; shift
  local name role cores; name=$(basename "$lab")
  ( cd "$lab" && env "$@" LIST=1 ./run_all.sh ) | sort -k2,2n -k1,1 > "$lab/pairs-all.txt"
  [ -s "$lab/pairs-all.txt" ] || die "$lab/run_all.sh lists no runs with LIST=1"
  rm -f "$lab"/pairs-f*.txt "$lab"/pairs-job.txt "$lab"/pairs-b.txt "$lab/pairs-local.txt"
  # Deal the runs to the worker that has the least of that run's arm per core, then
  # the least work per core: every worker gets each arm in proportion to its cores,
  # so that an arm is not tied to a machine.
  { fleet | cut -d' ' -f1,2; if [ -n "${AVIDA_CLOUD_LOCAL:-}" ]; then echo "local $AVIDA_CLOUD_LOCAL"; fi; } | python3 -c '
import sys
lab = sys.argv[1]
workers = [l.split() for l in sys.stdin if l.strip()]
if not workers: sys.exit("cloud: no workers")
load = {w: 0 for w, _ in workers}; cores = {w: int(c) for w, c in workers}; out = {w: [] for w, _ in workers}
arm_load = {}
for line in open(lab + "/pairs-all.txt"):
    al = arm_load.setdefault(line.split()[0], {w: 0 for w in load})
    w = min(load, key=lambda w: ((al[w] + 1) / cores[w], (load[w] + 1) / cores[w], w))
    load[w] += 1; al[w] += 1; out[w].append(line)
for w, lines in out.items():
    open(f"{lab}/pairs-{w}.txt", "w").writelines(lines)
    print(f"cloud: {w}: {len(lines)} runs on {cores[w]} cores")
' "$lab"
  while read -r role cores _; do
    [ -s "$lab/pairs-$role.txt" ] || continue
    ( set_role "$role"; run "$lab" "$@" "JOBS=$cores" "PAIRS_FILE=pairs-$role.txt" ) < /dev/null
  done < <(fleet)
  if [ -s "$lab/pairs-local.txt" ]; then
    echo "cloud: this machine's share, to start here:"
    echo "  cd $lab && env $* JOBS=$AVIDA_CLOUD_LOCAL PAIRS_FILE=pairs-local.txt caffeinate -i nohup ./run_all.sh > batch-local.log 2>&1 &"
  fi
}

fleet_status() {
  local role
  for role in $(fleet | cut -d' ' -f1); do echo "== $role"; ( set_role "$role"; status ) 2>&1 | sed 's/^/   /'; done
  [ -n "$(fleet)" ] || echo "cloud: no servers"
}

fleet_fetch() { # LABDIR: the runs of every server into the lab folder itself
  local lab=${1:?lab folder}; local name role; name=$(basename "$lab")
  for role in $(fleet | cut -d' ' -f1); do
    ( set_role "$role"
      rsync_to -azm --exclude 'arms/' --exclude 'resource_*.m' --include '*/' --include 'runs*/***' --include '**/runs*/***' --exclude '*' "HOST:/root/lab/$name/" "$lab/"
      rsync_to -az "HOST:/root/lab/$name/batch.log" "$lab/batch-$role.log" 2> /dev/null || true
      echo "cloud: $role: $(remote "find /root/lab/$name -name .done | wc -l; pgrep -xc avida" | tr '\n' ' ')(runs done, running)" )
  done
  echo "cloud: $(find "$lab" -name .done | wc -l | tr -d ' ') finished runs in $lab, of $(wc -l < "$lab/pairs-all.txt" | tr -d ' ') listed"
}

down() {
  [ "${1:-}" = --yes ] || die "down deletes the server and everything on it: cloud.sh down --yes"
  local id
  local sel="$SEL,role=$ROLE"; [ "${2:-}" = all ] && sel=$SEL
  for id in $(hz GET "/servers?$sel" | jq -r '.servers[].id'); do hz DELETE "/servers/$id" > /dev/null; echo "cloud: deleted server $id"; done
}

purge() {
  [ "${1:-}" = --yes ] || die "purge deletes the server, the firewall and the registered key: cloud.sh purge --yes"
  down --yes all; sleep 5
  local id
  for id in $(hz GET "/firewalls?$SEL" | jq -r '.firewalls[].id'); do hz DELETE "/firewalls/$id" > /dev/null; echo "cloud: deleted firewall $id"; done
  for id in $(hz GET "/ssh_keys?$SEL" | jq -r '.ssh_keys[].id'); do hz DELETE "/ssh_keys/$id" > /dev/null; echo "cloud: deleted key $id"; done
}

cmd=${1:-}; shift || true
case "$cmd" in
  up) up ;; build) build ;; run) run "$@" ;; status) status ;; fetch) fetch "$@" ;;
  ssh) remote "$@" ;; down) down "$@" ;; purge) purge "$@" ;;
  fleet-up) fleet_up ;; fleet-run) fleet_run "$@" ;; fleet-status) fleet_status ;; fleet-fetch) fleet_fetch "$@" ;;
  *) sed -n '2,26p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
