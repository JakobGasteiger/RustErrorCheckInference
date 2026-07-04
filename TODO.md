
Support multiple if brancehs especially for rc() in libssh

support bool returning subfunctions/methods

support return types other than bool and res/opt, like Openssl/x509/mod.rs/CrlStatus::from_ffi_status ?

support assertions?

always output span, not just name

Assignments, not just let stmts
X509_PURPOSE_get_idBIO_setr

Finding of empty checks : see also LibSSH::sftp.rs::812, LibSSH::channel.rs::340, a lot of indeterminate funcions in OpenSSL

Fix Ok/Err as function call : also see LibSSH::channel.rs::340

Explicitly only allow wrappers that are c_int (or any int?) -> Result or (possibly) Option