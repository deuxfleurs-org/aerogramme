use std::net::SocketAddr;

/// Seek compatibility with the Dovecot Authentication Protocol
///
/// ## Trace
///
/// ```text
/// S: VERSION	1	2
/// S: MECH	PLAIN	plaintext
/// S: MECH	LOGIN	plaintext
/// S: SPID	15
/// S: CUID	17654
/// S: COOKIE	f56692bee41f471ed01bd83520025305
/// S: DONE
/// C: VERSION	1	2
/// C: CPID	1
/// C: AUTH	2	PLAIN	service=smtp	
/// S: CONT	2	
/// C: CONT	2   base64string==	
/// S: OK	2	user=alice@example.tld
/// ```
///
/// ## Dovecot References
///
/// https://doc.dovecot.org/developer_manual/design/auth_protocol/
/// https://doc.dovecot.org/configuration_manual/authentication/authentication_mechanisms/#authentication-authentication-mechanisms
/// https://doc.dovecot.org/configuration_manual/howto/simple_virtual_install/#simple-virtual-install-smtp-auth
/// https://doc.dovecot.org/configuration_manual/howto/postfix_and_dovecot_sasl/#howto-postfix-and-dovecot-sasl

pub struct AuthServer {
    bind_addr: SocketAddr,
}
