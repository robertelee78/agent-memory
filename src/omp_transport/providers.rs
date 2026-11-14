//! Registered implementations: the boundary and stream transport seams, the
//! installed observer scope and the T2f half of the OMP activation interface.
use super::inventory::{list_pending, pending_inventory};
use super::observe::{Observation, ObserverScope, context_value, drain_after_response, observe};
use super::{Site, all_segments, err, lexists, native_disposition_ids};
use crate::codec::json::Value;
use crate::error::{ContractError, Result};
use crate::lifecycle::string;
use crate::omp_boundary;
use crate::omp_installation::{
    self, HeldSource, NativeDiagnostics, ObserverBinding, StreamDiagnostics,
};
use crate::omp_projection::same;
use crate::omp_stream::{self, FrameProof, Installed, ObserveCall, ObserveError};
use crate::scope_binding::{self, PrivateEnrollmentContext};
use crate::turn_sources::{self, SourceBarrier};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn installation_error(error: omp_installation::Error) -> ContractError {
    match error {
        omp_installation::Error::Contract(error) => error,
        omp_installation::Error::MissingActivation => {
            ContractError::missing_provider(omp_installation::Error::MissingActivation.to_string())
        }
    }
}

/// The installed observer scope: the frame's verified executable binding
/// (`omp_installation::observe_observer_binding`) around the marker, with the
/// live private enrollment context minted under that held scope.
pub struct InstalledScope<'a> {
    pub receipt: Option<&'a Value>,
    pub proof: &'a FrameProof,
    pub runtime_root: &'a Path,
}
impl ObserverScope for InstalledScope<'_> {
    fn observe(
        &self,
        repo: &Path,
        held: Option<&SourceBarrier>,
        body: &mut dyn FnMut(&PrivateEnrollmentContext) -> Result<()>,
    ) -> Result<()> {
        let receipt = self
            .receipt
            .filter(|receipt| receipt.as_object().is_some())
            .ok_or_else(|| err("Native observation requires a current installed binding"))?;
        let proof = self
            .proof
            .downcast_ref::<ObserverBinding>()
            .ok_or_else(|| err("Native observer requires its exact internally verified binding"))?;
        let held = held.map(|barrier| barrier as &dyn HeldSource);
        omp_installation::observe_observer_binding(repo, receipt, proof, held, |observed| {
            // The marker scope hands a context value; the transport needs the
            // live context itself. Mint it inside the same held scope and
            // require the identical root, bank, project, scope and hold.
            let context = scope_binding::private_enrollment_context(repo, self.runtime_root)?;
            if !same(&context_value(&context)?, observed) {
                return Err(
                    err("Native observer enrollment context changed during verification").into(),
                );
            }
            scope_binding::private_enrollment_reads(&context, |context| body(context))
                .map_err(omp_installation::Error::Contract)
        })
        .map_err(installation_error)
    }
}

/// The one `omp_transport` provider of both the boundary and the stream seams.
pub struct Transport {
    runtime_root: Option<PathBuf>,
}
impl Transport {
    /// The running installation's runtime root (`cli::pack_root`).
    pub fn installed() -> Self {
        Self { runtime_root: None }
    }
    /// An explicit runtime root.
    pub fn at(runtime_root: &Path) -> Self {
        Self {
            runtime_root: Some(runtime_root.to_path_buf()),
        }
    }
    fn runtime_root(&self) -> Result<PathBuf> {
        match &self.runtime_root {
            Some(root) => Ok(root.clone()),
            None => crate::cli::pack_root(),
        }
    }
}

/// `pending_inventory(repo)` for omp_boundary: `(rows, faults)`.
pub(crate) fn boundary_pending_inventory(
    repo: &Path,
    runtime_root: &Path,
) -> Result<(Vec<Value>, Vec<Value>)> {
    let (rows, faults) = pending_inventory(&Site::new(repo, runtime_root))?;
    Ok((rows, faults.into_iter().map(string).collect()))
}
/// `all_segments(repo)` for omp_boundary: `(segment directory, cursor)` pairs.
pub(crate) fn boundary_all_segments(
    repo: &Path,
    runtime_root: &Path,
) -> Result<Vec<(PathBuf, Value)>> {
    all_segments(&Site::new(repo, runtime_root), None)?
        .into_iter()
        .map(|segment| Ok((segment.directory.clone(), segment.snapshot()?)))
        .collect()
}
/// omp_boundary's `--source` read: session and lifecycle are required.
pub(crate) fn boundary_source(
    repo: &Path,
    session: Option<&str>,
    lifecycle: Option<&str>,
    identifier: &str,
    runtime_root: &Path,
) -> Result<Value> {
    let session = session.ok_or_else(|| err("Invalid bounded source identifier"))?;
    let lifecycle = lifecycle.ok_or_else(|| err("Use a canonical native lifecycle UUID"))?;
    super::inventory::source(repo, runtime_root, session, lifecycle, identifier)
}

impl omp_stream::OmpTransport for Transport {
    fn observe(
        &self,
        repo: &Path,
        request: &Value,
        call: ObserveCall<'_, '_>,
    ) -> std::result::Result<Value, ObserveError> {
        let runtime_root = self.runtime_root()?;
        let scope = InstalledScope {
            receipt: call.receipt,
            proof: call.verified_binding,
            runtime_root: &runtime_root,
        };
        let observation = Observation {
            receipt: call.receipt,
            on_started: Some(call.on_started),
            instance_id: Some(call.instance_id),
            identity: Some(call.identity),
            delivery_window: call.delivery_window,
            source_completion: call.source_completion,
            held_source: call.held_source,
        };
        Ok(observe(repo, &runtime_root, request, observation, &scope)?)
    }
    fn drain_after_response(
        &self,
        repo: &Path,
        receipt: Option<&Value>,
        proof: &FrameProof,
    ) -> Result<()> {
        let runtime_root = self.runtime_root()?;
        let scope = InstalledScope {
            receipt,
            proof,
            runtime_root: &runtime_root,
        };
        drain_after_response(repo, &runtime_root, &scope);
        Ok(())
    }
    fn native_disposition_ids(&self, repo: &Path, session: &str) -> Result<BTreeSet<String>> {
        let runtime_root = self.runtime_root()?;
        native_disposition_ids(&Site::new(repo, &runtime_root), session)
    }
}

/// The T2f half of `omp_installation::OmpActivation`, composed with the
/// installation half by `installation::InstallationActivation`.
pub struct Harness {
    pack_root: PathBuf,
}
impl Harness {
    pub fn at(pack_root: &Path) -> Self {
        Self {
            pack_root: pack_root.to_path_buf(),
        }
    }
    pub fn installed() -> omp_installation::Result<Self> {
        Ok(Self::at(&crate::cli::pack_root()?))
    }
}

/// Keep CLI registration lazy: an initial installation has no owned pack yet.
pub struct InstalledHarness;
impl crate::installation::OmpHarness for InstalledHarness {
    fn diagnostics(&self, repo: &Path) -> omp_installation::Result<NativeDiagnostics> {
        crate::installation::OmpHarness::diagnostics(&Harness::installed()?, repo)
    }
    fn observe_marker(
        &self,
        repo: &Path,
        held: Option<&dyn HeldSource>,
        operation: &mut dyn FnMut(&Value) -> omp_installation::Result<()>,
    ) -> omp_installation::Result<()> {
        crate::installation::OmpHarness::observe_marker(
            &Harness::installed()?,
            repo,
            held,
            operation,
        )
    }
    fn instruction_uri(&self, relative: &str) -> Option<String> {
        crate::omp_projection::instruction_uri(relative)
    }
}

fn stream_diagnostics(
    repo: &Path,
    base: &Path,
    runtime_root: &Path,
) -> Result<Option<StreamDiagnostics>> {
    if !lexists(&base.join("omp-stream-v1"))? && !lexists(&base.join("omp-stream-session-locks"))? {
        return Ok(None);
    }
    let authority = Installed::new(runtime_root);
    let streams = omp_stream::inventory(&authority, repo, Some(base), true)?;
    let instances = crate::lifecycle::get(&streams, "streams")
        .as_array()
        .map_or(0, <[Value]>::len);
    let pending = crate::lifecycle::get(&streams, "pending").clone();
    // Diagnostic-only wake failures use the unavailable summary.
    let wake_decisions = omp_stream::wake_decisions(&authority, repo, &streams, Some(base))
        .unwrap_or_else(|_| omp_stream::wake_decision_summary(false));
    Ok(Some(StreamDiagnostics {
        instances,
        pending,
        wake_decisions,
    }))
}

impl crate::installation::OmpHarness for Harness {
    fn diagnostics(&self, repo: &Path) -> omp_installation::Result<NativeDiagnostics> {
        let runtime_root = &self.pack_root;
        let diagnostics = turn_sources::read_only_locks(|| {
            let base = omp_boundary::base(repo)?;
            let pending = if lexists(&base.join("omp-source-v1"))? {
                Some(list_pending(repo, &runtime_root, 0)?)
            } else {
                None
            };
            let streams = stream_diagnostics(repo, &base, &runtime_root)?;
            let verified = (|| -> Result<()> {
                let barrier = turn_sources::source_barrier(repo, false, true, None, &runtime_root)?;
                omp_boundary::verify_marker(repo, &runtime_root)?;
                barrier.recheck()
            })()
            .is_ok();
            Ok(NativeDiagnostics {
                pending,
                streams,
                stored_source_verified: verified,
            })
        })?;
        Ok(diagnostics)
    }
    fn observe_marker(
        &self,
        repo: &Path,
        held: Option<&dyn HeldSource>,
        operation: &mut dyn FnMut(&Value) -> omp_installation::Result<()>,
    ) -> omp_installation::Result<()> {
        let runtime_root = &self.pack_root;
        match held {
            Some(held) => {
                omp_boundary::observe_marker_operation(repo, Some(held), &runtime_root, operation)?;
                held.recheck()
            }
            None => {
                let barrier =
                    turn_sources::source_barrier(repo, false, false, None, &runtime_root)?;
                omp_boundary::observe_marker_operation(repo, None, &runtime_root, operation)?;
                Ok(barrier.recheck()?)
            }
        }
    }
    fn instruction_uri(&self, relative: &str) -> Option<String> {
        crate::omp_projection::instruction_uri(relative)
    }
}
