// Posted-New whole-certificate observation only. An unreadable AXWindow query
// proves neither membership nor loss; only a later COMPLETE original proof can pass.
import Foundation
import ApplicationServices

func ui32NewFinalNativeBusy(_ error: Error) -> Bool {
  guard let outer = (error as? ActionRefusal)?.diagnostics,
    Set(outer.keys) == ["purpose", "stage", "proofStep", "underlying"],
    outer["purpose"] as? String == "UI32",
    outer["stage"] as? String == "content-read-proof",
    outer["proofStep"] as? String == "tree-final-native-identity",
    let cause = outer["underlying"] as? [String: Any],
    Set(cause.keys) == ["kind", "stage", "facts"],
    cause["kind"] as? String == "identity",
    cause["stage"] as? String == "application-window-membership",
    let facts = cause["facts"] as? [String: Any] else { return false }
  let booleans = ["applicationWindowsReadable": false, "axPidMatches": true,
    "minimized": false, "minimizedReadable": false, "pinnedAXWindowMember": false,
    "pinnedCGWindowListReadable": true]
  let numbers = ["applicationWindowsQueryError": Int(AXError.cannotComplete.rawValue),
    "axPidQueryError": 0, "minimizedQueryError": Int(AXError.cannotComplete.rawValue),
    "pinnedCGWindowCount": 1]
  guard Set(facts.keys) == Set(booleans.keys).union(numbers.keys) else { return false }
  for (key, expected) in booleans {
    guard let raw = facts[key], type(of: raw) == Bool.self,
      raw as? Bool == expected else { return false }
  }
  for (key, expected) in numbers {
    guard let raw = facts[key], type(of: raw) == Int.self,
      raw as? Int == expected else { return false }
  }
  return true
}

func ui32NewFinalNativeBusySelfTest() throws {
  let facts: [String: Any] = ["applicationWindowsQueryError": Int(AXError.cannotComplete.rawValue),
    "applicationWindowsReadable": false, "axPidMatches": true, "axPidQueryError": 0,
    "minimized": false, "minimizedQueryError": Int(AXError.cannotComplete.rawValue),
    "minimizedReadable": false, "pinnedAXWindowMember": false,
    "pinnedCGWindowCount": 1, "pinnedCGWindowListReadable": true]
  var actual: Error?
  do {
    let _: Void = try ui32ContentReadStep("tree-final-native-identity") {
      throw IdentityRefusal(stage: "application-window-membership", facts: facts)
    }
  } catch { actual = error }
  guard let actual, let produced = (actual as? ActionRefusal)?.diagnostics else {
    throw Refusal(code: "adapter-refused")
  }
  try check(ui32NewFinalNativeBusy(actual))
  for fault in ["purpose", "stage", "step", "kind", "identity-stage", "extra", "nested",
    "missing", "query", "query-type", "pid", "pid-query", "cg", "cg-query", "readable",
    "minimized-readable", "minimized-query", "member", "bool-type"] {
    var outer = produced, cause = produced["underlying"] as! [String: Any], changed = facts
    switch fault {
    case "purpose": outer["purpose"] = "other"
    case "stage": outer["stage"] = "read-proof"
    case "step": outer["proofStep"] = "tree-identity"
    case "kind": cause["kind"] = "action"
    case "identity-stage": cause["stage"] = "process-start"
    case "extra": changed["extra"] = true
    case "nested": outer = ["underlying": produced]
    case "missing": changed.removeValue(forKey: "applicationWindowsQueryError")
    case "query": changed["applicationWindowsQueryError"] = Int(AXError.invalidUIElement.rawValue)
    case "query-type": changed["axPidQueryError"] = false
    case "pid": changed["axPidMatches"] = false
    case "pid-query": changed["axPidQueryError"] = Int(AXError.cannotComplete.rawValue)
    case "cg": changed["pinnedCGWindowCount"] = 2
    case "cg-query": changed["pinnedCGWindowListReadable"] = false
    case "readable": changed["applicationWindowsReadable"] = true
    case "minimized-readable": changed["minimizedReadable"] = true
    case "minimized-query": changed["minimizedQueryError"] = Int(AXError.noValue.rawValue)
    case "member": changed["pinnedAXWindowMember"] = true
    default: changed["axPidMatches"] = 1
    }
    if fault != "nested" { cause["facts"] = changed; outer["underlying"] = cause }
    try check(!ui32NewFinalNativeBusy(ActionRefusal(diagnostics: outer)))
  }
  try check(!ui32NewFinalNativeBusy(IdentityRefusal(stage: "application-window-membership", facts: facts)))
  for fault in ["settled", "second-busy", "pre-identity", "retry-identity", "final-identity",
    "deadline", "late-success", "wrong-phase"] {
    var clock: UInt64 = 0, reads = 0, identities = 0, refused = false
    var deadlines = [UInt64](), proof = [String: Any]()
    do {
      proof = try ui32NewPostCreateRead("entry.create.default", true, true, true,
        fault == "wrong-phase" ? 1 : 2, now: { clock }, identity: {
          identities += 1
          if (fault == "pre-identity" && identities == 1) ||
            (fault == "retry-identity" && identities == 2) ||
            (fault == "final-identity" && identities == 3) { throw actual }
        }, read: { deadline in
          reads += 1; deadlines.append(deadline)
          if fault == "deadline" || (fault == "late-success" && reads == 2) { clock = deadline }
          if reads == 1 || fault == "second-busy" { throw actual }
        })
    } catch { refused = true }
    try check(refused == (fault != "settled"))
    let expected = ["wrong-phase", "pre-identity"].contains(fault) ? 0 :
      ["deadline", "retry-identity"].contains(fault) ? 1 : 2
    try check(reads == expected && Set(deadlines).count <= 1 &&
      deadlines.allSatisfy { $0 == 750_000_000 })
    if fault == "settled" {
      try check(proof["readAttempts"] as? Int == 2 && proof["firstReadFailure"] != nil &&
        proof["oneCreateActionOnly"] as? Bool == true && identities == 3)
    }
  }
}
