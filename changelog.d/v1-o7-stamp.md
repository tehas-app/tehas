### Changes

- `state/analytics.db` gains `runs.contractor_id`, the contractor that ran a cycle, inside schema 4: a file written by 0.30.x gains the column, empty, the first time it is opened, and `tehas analytics copy` carries it. It stays empty until outsource ships (tehas#2276).
