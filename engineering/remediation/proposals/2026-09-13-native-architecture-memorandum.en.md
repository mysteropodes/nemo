# Nemo: the foundation for its next stage

**Memorandum for Ilya and Cyrill · 13 September 2026**  
Based on the source and remediation review completed on this date.

Nemo should keep its Tauri/JavaScript interface and prepare its remaining remediation work for an independent native engine and viewport. This preserves useful work while giving the project a more credible path toward demanding creative workloads. The decision now is to establish that direction and test its feasibility. Delivering the complete engine is a separate commitment, with its own scope and estimate.

## Where the project stands

Nemo has substantial functionality, and its remediation has made real progress. Of the original 59 defined tasks, 41 are closed and 18 remain open. Completed work includes clearer responsibilities, safer application operations, tests, and separation of previously intertwined code.

That does not mean the whole remediation is 70% complete. The broader source inventory still needs to be reconciled with completed work and converted into an agreed remaining task list. Some inventory entries represent work already delivered; others require further separation or implementation. The remaining workload cannot yet be reduced to one reliable completion figure.

The current remediation is principally about making Nemo easier to understand, change, test, and extend. Its finish line allows existing defects and platform limitations to remain explicitly documented. Completing it would be valuable, but would not establish that Nemo can sustain After Effects or DaVinci Resolve workloads.

The architectural concern is that too much project evaluation and preparation for rendering still depends on the interface and its live objects. GPU acceleration already exists, but it does not remove the cost of preparing each frame, moving media between components, or coordinating editing and export.

## What the proposed architecture would change

The visible interface could remain largely familiar: panels, controls, timeline, and application chrome in Tauri/JavaScript. A native engine would own the project, evaluate animation, process media, and produce the viewport image. The interface would communicate editing instructions rather than participate in every stage of producing every frame.

This is more than moving rendering into a background worker. The engine needs its own coherent view of the project, control over memory and GPU resources, and a way to discard obsolete work when the user changes direction. The viewport also needs to display native output without repeatedly carrying full images through JavaScript.

A practical example is editing during export. The current guarded export detects project changes and stops rather than risk mixing different revisions. The target architecture would let export continue from a fixed project version while editing proceeds independently. That is a workflow benefit arising from ownership and scheduling, not simply from using Rust.

## The two choices

| | Finish remediation as currently defined | Adjust remediation toward the native architecture |
|---|---|---|
| Immediate benefit | A more maintainable and testable version of the current system. | The same structural progress, with upcoming boundaries designed for the intended engine. |
| Main advantage | Less disruption and fewer new architectural decisions before structural completion. | Less risk of making new code depend on assumptions that the migration must later remove. |
| Main cost | Some future work may need another pass when project ownership and rendering move. | Additional design, feasibility checks, and transition testing now; a separate implementation effort later. |
| Expected result at remediation completion | Better foundations for extending the present architecture. | Better foundations plus a tested migration direction. The production native engine is still to come. |
| Heavy-workload outlook | Performance improvements remain possible, but interface dependencies and data movement remain constraints. | A stronger route to sustained workloads once the native engine is implemented and validated. |

The unchanged route is reasonable if the near-term goal is a focused creative tool and the priority is completing structural work. It does not prevent a later migration. Its disadvantage is that further features may deepen dependencies on the current evaluation and rendering model, increasing the amount that must eventually change.

The adjusted route better matches a product expected to handle increasingly complex animation, substantial footage, and longer processing jobs. After the full migration, the intended outcomes would be more responsive interaction under load, more predictable memory use, independent background rendering, and closer agreement between preview and export.

Those are design objectives, not demonstrated results. Large compositions can still exceed available processing power. Cached frames, proxies, and reduced preview resolution would remain useful. Matching established professional applications also requires mature effects, media support, color handling, and extensive testing; architecture alone does not provide those capabilities.

The migration would introduce its own difficulties: integrating the native viewport with interface overlays and input, preserving existing projects and animation behavior, and validating supported desktop platforms. A browser version would need a clearly defined capability set. During the transition, maintaining both old and new paths would add work and could delay visible features.

## What this means for development with AI

For us, the value of this decision is how safely Nemo can become more sophisticated. AI makes it easier to add substantial code quickly. The harder question is whether a change to animation, history, export, or media remains consistent everywhere that behavior matters.

Clear ownership and small, explicit interfaces make AI-assisted work easier to divide and review. They reduce the amount of application context each task needs. Additional agents cannot compensate for two components disagreeing about the current project state, and passing code tests cannot establish that scrubbing feels right or an exported image is correct.

Our creative judgment remains part of acceptance: representative projects, timing, visual fidelity, editing responsiveness, and recovery from interrupted operations. These should guide the migration rather than code volume or a successful viewport demonstration.

## Cost and the next decision

The calibrated assessment puts the **additional preparation at approximately 12 hours expected, with an 18-hour stretch allowance**, using either one supported local team or the two existing teams on separate work. Solo execution is approximately 13 hours, with a 19.5-hour stretch allowance. Wider agent arrangements were excluded because they exceed the current operating limits.

This covers plan adjustments, compatible interfaces, explicit transition arrangements, and two limited feasibility checks. **It does not purchase the full native engine or prove professional performance.** That implementation remains separately scoped and unestimated.

The larger uncertainty in both scenarios is the still-unresolved remaining task list. The detailed assessment therefore gives conditional totals rather than a promised finish time. Its assumptions include available reviewers and validation machines; open-ended external waits are excluded.

The next step should be to reconcile that task list while testing native viewport integration and independent evaluation of a small existing project fixture. Those results would make the subsequent implementation decision concrete.

## Conclusion

Adopt the native engine and viewport as Nemo’s target now, while preserving accepted remediation work and the current interface. Adjust the upcoming boundaries before more work depends on the present rendering model. Complete the limited feasibility checks, then price and approve the actual migration against representative creative workloads.

For the increasingly sophisticated tools we want to build, this is the stronger direction. It preserves today’s investment and gives future development a clearer foundation. Its success must ultimately be judged by what we can comfortably create, preview, and deliver in Nemo.

---

References: [Execution plan](../EXECUTION_PLAN.en.md) and [current and target architecture](../reference/01_CURRENT_AND_TARGET.md). Status and estimates reflect the 13 September 2026 assessment of main `9f523462c201740b7f6805ab7bc6c886571fdf30`; this is a dated decision memorandum, not a live progress report. This memorandum proposes a direction; it does not amend the execution plan or authorize implementation.
