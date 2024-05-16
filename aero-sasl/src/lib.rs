pub mod decode;
pub mod encode;
pub mod flow;
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
///
/// C: AUTH	2	PLAIN	service=smtp
/// S: CONT	2
/// C: CONT	2   base64stringFollowingRFC4616==
/// S: OK	2	user=alice@example.tld
///
/// C: AUTH	42	LOGIN	service=smtp
/// S: CONT	42	VXNlcm5hbWU6
/// C: CONT	42	b64User
/// S: CONT	42	UGFzc3dvcmQ6
/// C: CONT	42	b64Pass
/// S: FAIL	42	user=alice
/// ```
///
/// ## RFC References
///
/// PLAIN SASL - https://datatracker.ietf.org/doc/html/rfc4616
///
///
/// ## Dovecot References
///
/// https://doc.dovecot.org/developer_manual/design/auth_protocol/
/// https://doc.dovecot.org/configuration_manual/authentication/authentication_mechanisms/#authentication-authentication-mechanisms
/// https://doc.dovecot.org/configuration_manual/howto/simple_virtual_install/#simple-virtual-install-smtp-auth
/// https://doc.dovecot.org/configuration_manual/howto/postfix_and_dovecot_sasl/#howto-postfix-and-dovecot-sasl
pub mod types;
