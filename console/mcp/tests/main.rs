// The MCP server's integration tests as one binary: nine modules, the tests'
// names unchanged. Each file is linked as a module of this one instead of as a
// test target of its own, so the workspace links one copy of the test harness
// and the server's code rather than nine (issue #1116).
//
// cargo runs the test binaries one after another and the threads of one binary
// side by side, so the tests of nine files now run at once. Each takes its own
// directory under the temporary directory, named after itself, and binds no
// port: the server's tests ask for port 0 and read the port back.
mod decisions;
mod events;
mod logs;
mod outcomes;
mod park;
mod rpc;
mod serve;
mod sessions;
mod tools;
