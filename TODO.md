
support bool returning subfunctions/methods 

support return types other than bool and res/opt, like Openssl/x509/mod.rs/CrlStatus::from_ffi_status ? 

support assertions? 

always output span, not just name 

Assignments, not just let stmts ? 

Explicitly only allow wrappers that take c_int (or any int?) as input from wrapped function? 

support for match guards that are not binary expressions? (in particular hardcoded methods)

support "reversed" binops

LibSSH::channel.rs::554 god example of what is too complex to support fully? (result still right in this case)