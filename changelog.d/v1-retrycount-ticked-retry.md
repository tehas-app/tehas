### Changes

- A Retry ticked in a park comment now counts as waiting work in `demand.waiting`, so a program that holds the limit at 0 while nothing waits raises it and the retry starts (#1776).
