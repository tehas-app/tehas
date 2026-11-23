### Changes

- Every cycle start now raises one `cycle.ended`, also when the start is refused before its runner runs, when `tehas stop` ends the record, and when a cycle is ended as lost by the poll or by the next start. The process that wrote the cycle's line of history raises it, so a late report or stop raises no second end. The dispatcher raises `question.asked` for each question it records, a planning turn's among them. `tehas stop` writes its event line to `logs/dispatch.log` (#2940).
