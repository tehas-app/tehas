# The shape of one row of the state store's history, as `tehas history` lists
# it: a cycle's end in the shape its line had before the store, and a planning
# turn's end with its outcome as status and its exit status (or the outcome
# when the unit left none) as rc. A key the row does not have is left out,
# never null.
#
# Needs $repo, the instance's repository, for a pull request's address, and
# $kinds: the rows to list, as a jq array of the kind names of the store
# ("cycle" and "plan" together, as tehas history lists them). lib/millwright.sh
# reads the file with jq -f; it holds no bash. The console has its own copy
# (console/src/history.rs), and a test there holds the two to the same lines.
def history_line:
  {at: (.at | todate), started: .started_at, rc, status, step, cap, park_reason, failure_kind,
   failure_cause, provider, parked,
   pr: (if .pr == null then null else "https://github.com/\($repo)/pull/\(.pr)" end), by, how}
  | with_entries(select(.value != null and .value != ""));
.rows[]
| if .kind == "cycle" and ($kinds | index("cycle")) != null then {issue, kind: "work", seq} + history_line
  elif .kind == "plan" and ($kinds | index("plan")) != null then
    {issue, kind: "plan", seq, at: (.at | todate), started: .started_at, rc: (.rc // .outcome), status: .outcome,
     turn, by, label, filed, auto: (if .auto == true then true else null end)}
    | with_entries(select(.value != null and .value != "" and .value != []))
  else empty end
