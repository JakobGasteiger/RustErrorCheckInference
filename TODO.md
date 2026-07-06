
Support multiple if brancehs especially for rc() in libssh

support bool returning subfunctions/methods

support return types other than bool and res/opt, like Openssl/x509/mod.rs/CrlStatus::from_ffi_status ?

support assertions?

always output span, not just name

Assignments, not just let stmts ?

Fix Ok/Err as function call : also see LibSSH::channel.rs::340

Explicitly only allow wrappers that are c_int (or any int?) -> Result or (possibly) Option

support match stmts that dont use guards or that have more than one arm

support if stmts with elif