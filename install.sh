#!/bin/sh
# Agent Memory bootstrap. Downloads the signed `am` binary for this host,
# verifies it with ssh-keygen against the release key pinned below, and only
# then runs `am install`. POSIX sh only; no interpreter and no JSON parsing.
# The whole program is one compound command, so a truncated download cannot
# execute a complete prefix of it.
if :; then
set -eu

# Release constants. Nothing in the environment or on the command line can
# change them; the contract tests substitute these lines in a private copy.
AM_RELEASE_BASE='https://github.com/robertelee78/agent-memory/releases/latest/download/'
AM_PROTOCOLS='=https'
AM_ALLOWED_SIGNERS='release@agent-memory namespaces="agent-memory.release,agent-memory.release-binary" ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB+C//uSRKmScS5hTki796yvLR2oOSn2IKHLkr1RWE2y'
AM_PRINCIPAL='release@agent-memory'
AM_NAMESPACE='agent-memory.release-binary'
AM_BINARY_LIMIT=67108864
AM_SIGNATURE_LIMIT=4096
AM_ATTEMPTS=3
AM_ATTEMPT_SECONDS=60
AM_CONNECT_SECONDS=5

am_os=other
am_arch=other
am_triple=
am_directory=

# One fixed JSON line: constant strings chosen from the tables in this file
# plus integer curl exit / HTTP status. Never text from the host or network.
am_fail() {
  printf '{"schema":"agent-memory/bootstrap-failure-v1","stage":"%s","reason":"%s","os":"%s","arch":"%s","curl_exit":%s,"http_status":%s,"activation_accepted":false,"repair":"%s"}\n' \
    "$1" "$2" "$am_os" "$am_arch" "$4" "$5" "$6"
  exit "$3"
}

am_refuse() {
  am_fail "$1" "$2" "$3" null null "$4"
}

am_help() {
  printf '%s\n' \
    'Install Agent Memory: install.sh [--check|--dry-run] [--json] [--verbose] [--mode link|copy] [--presets all|none|CLIENTS]' \
    'Custom discovery: --target-dir PATH, --bin-dir PATH, --no-guidance' \
    'Native omp (excluded from all): --presets omp --omp-agent-dir ABSOLUTE_ROOT --omp-peers-upgraded' \
    'Quiescence gates: --layout-quiescence VALUE, --omp-layout-quiescence VALUE' \
    'Explicit sources: --development-source PATH or --remote URL --ref BRANCH' \
    'Bootstrap: downloads the signed am binary for this host, verifies it with ssh-keygen, then runs am install.' \
    'Requires curl and ssh-keygen. Hosts: aarch64-apple-darwin, x86_64-apple-darwin, x86_64-unknown-linux-musl, aarch64-unknown-linux-musl.' \
    'After installation: am --version; am update; am uninstall'
}

# Syntax check of the options `am install` accepts, so a typo needs no download.
am_validate() {
  am_development=no
  am_channel=no
  while [ "$#" -gt 0 ]; do
    am_word=$1
    case $am_word in
      --remote=*|--ref=*|--development-source=*|--mode=*|--presets=*|--target-dir=*|--bin-dir=*|--omp-agent-dir=*|--omp-layout-quiescence=*|--layout-quiescence=*)
        am_option=${am_word%%=*}
        am_value=${am_word#*=}
        ;;
      --remote|--ref|--development-source|--mode|--presets|--target-dir|--bin-dir|--omp-agent-dir|--omp-layout-quiescence|--layout-quiescence)
        if [ "$#" -lt 2 ]; then
          am_refuse usage missing-value 2 'An option needs a value. Run install.sh --help.'
        fi
        am_option=$am_word
        shift
        am_value=$1
        ;;
      --check|--dry-run|--no-guidance|--json|--verbose|-v|--omp-peers-upgraded)
        am_option=$am_word
        am_value=flag
        ;;
      *)
        am_refuse usage unknown-option 2 'Unknown install option. Run install.sh --help.'
        ;;
    esac
    case $am_value in
      ''|--*)
        am_refuse usage missing-value 2 'An option needs a value. Run install.sh --help.'
        ;;
    esac
    case $am_option in
      --development-source) am_development=yes ;;
      --remote|--ref) am_channel=yes ;;
    esac
    shift
  done
  if [ "$am_development" = yes ] && [ "$am_channel" = yes ]; then
    am_refuse usage conflicting-sources 2 'Choose development source or managed remote/ref, not both.'
  fi
}

am_detect() {
  am_kernel=$(uname -s 2>/dev/null) || am_kernel=
  am_machine=$(uname -m 2>/dev/null) || am_machine=
  case $am_kernel in
    Darwin) am_os=darwin ;;
    Linux) am_os=linux ;;
    FreeBSD) am_os=freebsd ;;
    OpenBSD) am_os=openbsd ;;
    NetBSD) am_os=netbsd ;;
    DragonFly) am_os=dragonfly ;;
    SunOS) am_os=sunos ;;
    AIX) am_os=aix ;;
    Haiku) am_os=haiku ;;
    CYGWIN*|MINGW*|MSYS*) am_os=windows ;;
    *) am_os=other ;;
  esac
  case $am_machine in
    arm64|aarch64) am_arch=aarch64 ;;
    x86_64|amd64) am_arch=x86_64 ;;
    i386|i486|i586|i686|i86pc) am_arch=x86 ;;
    armv*) am_arch=arm ;;
    riscv64) am_arch=riscv64 ;;
    ppc64le) am_arch=ppc64le ;;
    ppc64) am_arch=ppc64 ;;
    s390x) am_arch=s390x ;;
    *) am_arch=other ;;
  esac
  case $am_os/$am_arch in
    darwin/aarch64) am_triple=aarch64-apple-darwin ;;
    darwin/x86_64) am_triple=x86_64-apple-darwin ;;
    linux/x86_64) am_triple=x86_64-unknown-linux-musl ;;
    linux/aarch64) am_triple=aarch64-unknown-linux-musl ;;
    *) am_triple= ;;
  esac
}

am_workspace() {
  case ${TMPDIR:-} in
    /*) am_parent=${TMPDIR%/} ;;
    *) am_parent=/tmp ;;
  esac
  umask 077
  am_directory=$(mktemp -d "$am_parent/am-bootstrap.XXXXXXXX" 2>/dev/null) || am_directory=
  if [ -z "$am_directory" ] || [ ! -d "$am_directory" ] || [ -h "$am_directory" ]; then
    am_refuse workspace temporary-directory 1 'Could not create a private temporary directory. Check TMPDIR space and permissions.'
  fi
  chmod 700 "$am_directory"
}

# Baseline download-failure-v1 vocabulary for curl exits.
am_category() {
  if [ "$am_reason" != transport ]; then
    am_category=$am_reason
    return 0
  fi
  case $am_code in
    5) am_category='proxy DNS failure' ;;
    6) am_category='DNS failure' ;;
    7) am_category='connection failure' ;;
    16) am_category='HTTP/2 failure' ;;
    18) am_category='incomplete response' ;;
    22) am_category='HTTP rejection' ;;
    23) am_category='local staging write failure' ;;
    28) am_category='transfer timeout' ;;
    35) am_category='TLS handshake failure' ;;
    52) am_category='empty response' ;;
    55) am_category='send failure' ;;
    56) am_category='receive failure' ;;
    60) am_category='certificate validation failure' ;;
    77) am_category='certificate configuration failure' ;;
    92) am_category='HTTP/2 stream failure' ;;
    *) am_category='transport failure' ;;
  esac
}

am_guidance() {
  am_guidance='Failed downloaded bytes were not accepted for activation. '
  if [ "$am_reason" = size-limit ]; then
    am_guidance=$am_guidance'Check expected release size and local temporary-storage/file-size limits; report a persistent mismatch.'
    return 0
  fi
  case $am_code in
    35|51|58|59|60|64|66|77|82|83|90|91)
      am_guidance=$am_guidance'Check the system clock, trusted certificates and any TLS-inspecting proxy configuration, then retry.'
      return 0
      ;;
  esac
  case $am_http in
    429)
      am_guidance=$am_guidance'The server rate-limited this request. Wait for its limit to reset, then retry the same command.'
      return 0
      ;;
    401|403)
      am_guidance=$am_guidance'Check server access restrictions, proxy policy and GitHub API rate limits, then retry.'
      return 0
      ;;
    404)
      am_guidance=$am_guidance'The release endpoint or asset is unavailable. Check the selected release or report the missing asset.'
      return 0
      ;;
  esac
  if [ "$am_code" -eq 23 ] || [ "$am_reason" = process-start ]; then
    am_guidance=$am_guidance'Check that curl is executable and private temporary storage has space and writable permissions.'
    return 0
  fi
  am_guidance=$am_guidance'Retry the same command after checking network, DNS, VPN/proxy access and connection speed to the reported host.'
}

# Bounded download with the baseline retry and failure classification.
am_download() {
  am_url=$1
  am_output=$2
  am_limit=$3
  am_stage=$4
  am_blocks=$((am_limit / 512 + 2))
  am_attempt=1
  while :; do
    rm -f -- "$am_output"
    am_code=0
    am_status=$(
      ulimit -f "$am_blocks" 2>/dev/null || :
      exec curl --disable --fail --silent --location \
        --proto "$AM_PROTOCOLS" --proto-redir "$AM_PROTOCOLS" --max-redirs 5 \
        --max-time "$AM_ATTEMPT_SECONDS" --connect-timeout "$AM_CONNECT_SECONDS" \
        --max-filesize "$am_limit" --header 'Accept: application/octet-stream' \
        --output "$am_output" --write-out '%{http_code}' -- "$am_url" 2>/dev/null
    ) || am_code=$?
    am_http=null
    case $am_status in
      [1-5][0-9][0-9]) am_http=$am_status ;;
    esac
    am_size=0
    if [ -f "$am_output" ]; then
      am_size=$(($(wc -c < "$am_output")))
    fi
    am_reason=transport
    if [ "$am_size" -gt "$am_limit" ] || [ "$am_code" -eq 63 ] || [ "$am_code" -eq 153 ]; then
      am_reason=size-limit
    elif [ "$am_code" -eq 126 ] || [ "$am_code" -eq 127 ]; then
      am_reason=process-start
    elif [ "$am_code" -gt 128 ]; then
      am_reason=process-terminated
    fi
    if [ "$am_code" -eq 0 ] && [ "$am_reason" = transport ]; then
      return 0
    fi
    am_retry=no
    if [ "$am_reason" = transport ]; then
      case $am_code in
        5|6|7|16|18|28|52|55|56|92)
          if [ "$am_http" = null ] || [ "$am_http" -lt 400 ]; then
            am_retry=yes
          fi
          ;;
        22)
          case $am_http in
            408|500|502|503|504) am_retry=yes ;;
          esac
          ;;
      esac
    fi
    if [ "$am_retry" = no ] || [ "$am_attempt" -ge "$AM_ATTEMPTS" ]; then
      break
    fi
    sleep "$am_attempt"
    am_attempt=$((am_attempt + 1))
  done
  rm -f -- "$am_output"
  am_category
  am_guidance
  am_fail "$am_stage" "$am_category" 1 "$am_code" "$am_http" "$am_guidance"
}

am_verify() {
  am_signers=$am_directory/allowed-signers
  printf '%s\n' "$AM_ALLOWED_SIGNERS" > "$am_signers"
  if ssh-keygen -Y verify -f "$am_signers" -I "$AM_PRINCIPAL" -n "$AM_NAMESPACE" -s "$am_signature" < "$am_binary" > /dev/null 2>&1; then
    return 0
  fi
  am_refuse signature-verification invalid-signature 1 'The downloaded am binary is not signed by the pinned release key. Nothing was run; retry, and report a persistent failure.'
}

for am_argument in "$@"; do
  case $am_argument in
    --help|-h)
      am_help
      exit 0
      ;;
  esac
done

# The private directory is removed on every exit path, including signals.
trap 'if [ -n "$am_directory" ]; then rm -rf -- "$am_directory"; fi' EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

am_detect
am_validate "$@"
if [ -z "$am_triple" ]; then
  am_refuse target unsupported-target 1 'No signed am binary exists for this host. Supported: aarch64-apple-darwin, x86_64-apple-darwin, x86_64-unknown-linux-musl, aarch64-unknown-linux-musl.'
fi
if ! command -v curl > /dev/null 2>&1; then
  am_refuse dependency missing-curl 1 'curl is required to download the signed am binary. Install curl, then retry.'
fi
if ! command -v ssh-keygen > /dev/null 2>&1; then
  am_refuse dependency missing-ssh-keygen 1 'OpenSSH ssh-keygen with SSH signature support is required. Install OpenSSH, then retry.'
fi
am_workspace
am_binary=$am_directory/am-$am_triple
am_signature=$am_binary.sshsig
am_download "$AM_RELEASE_BASE""am-$am_triple" "$am_binary" "$AM_BINARY_LIMIT" binary-download
am_download "$AM_RELEASE_BASE""am-$am_triple.sshsig" "$am_signature" "$AM_SIGNATURE_LIMIT" signature-download
am_verify
chmod 700 "$am_binary"
am_exit=0
"$am_binary" install --bootstrap-binary "$am_binary" "$@" || am_exit=$?
exit "$am_exit"
fi
