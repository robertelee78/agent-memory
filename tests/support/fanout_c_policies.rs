const POLICIES: [&str; 8] = [
    "In the synthetic offline-queue environment, attempt delivery at most three times, retain every queued item after failure, and resume delivery only after the operator changes the environment to online.",
    "For synthetic read-only database snapshots, pin the snapshot revision before querying, reject any write request, and discard the query result if the revision changes before readback.",
    "For the synthetic local cache, a missing checksum is a cache miss rather than an empty value; preserve the backing file and rebuild only after reading the authoritative fixture input.",
    "For synthetic batch imports, process records in original sequence, keep the first invalid record pending with its original ordinal, and never report the unread remainder as imported.",
    "For synthetic dry-run deployments, generate the exact target plan without activating it; activation requires a separate operator decision tied to that same plan and target revision.",
    "For synthetic expired credentials, stop the requested network operation without retrying another account; retain the pending request and ask the detecting developer to renew the intended account.",
    "For synthetic concurrent edits, preserve both source versions and their conditions; a later timestamp alone does not choose the correct meaning or authorize overwriting either version.",
    "For synthetic terminal deletion, retain only the content-free stable-identity marker in current memory and reject any later stale snapshot that tries to resurrect the removed body.",
];
