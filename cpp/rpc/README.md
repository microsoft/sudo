# Sudo RPC library

To do local RPC, we need to use midl to generate function bindings for our RPC
interface, which is defined in `sudo_rpc.idl`. midl expects implementations and
callbacks via C functions which we can do in Rust, but there's one problem:
Error handling on the client side occurs with structed exceptions (SEH).
Those cannot be easily replicated in pure Rust and so the client side calls
are all wrapped in C functions.

Changes here go as follows:
* Change the interface in `sudo_rpc.idl`
* Write a client-side wrapper in `RpcClient.c` in the style of the other ones
* Implement a client-side wrapper Rust in `rpc_bindings_client.rs`
* Implement the server-side part in `rpc_bindings_server.rs`

Be careful about the function definitions. At the moment, there are no checks
in place that ensure that the Rust code matches the C code or the .idl file.
