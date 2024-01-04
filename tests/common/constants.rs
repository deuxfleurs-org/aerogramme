use std::time;

pub static SMALL_DELAY: time::Duration = time::Duration::from_millis(200);

pub static EMAIL1: &[u8] = b"Date: Sat, 8 Jul 2023 07:14:29 +0200\r
From: Bob Robert <bob@example.tld>\r
To: Alice Malice <alice@example.tld>\r
CC: =?ISO-8859-1?Q?Andr=E9?= Pirard <PIRARD@vm1.ulg.ac.be>\r
Subject: =?ISO-8859-1?B?SWYgeW91IGNhbiByZWFkIHRoaXMgeW8=?=\r
    =?ISO-8859-2?B?dSB1bmRlcnN0YW5kIHRoZSBleGFtcGxlLg==?=\r
X-Unknown: something something\r
Bad entry\r
  on multiple lines\r
Message-ID: <NTAxNzA2AC47634Y366BAMTY4ODc5MzQyODY0ODY5@www.grrrndzero.org>\r
MIME-Version: 1.0\r
Content-Type: multipart/alternative;\r
 boundary=\"b1_e376dc71bafc953c0b0fdeb9983a9956\"\r
Content-Transfer-Encoding: 7bit\r
\r
This is a multi-part message in MIME format.\r
\r
--b1_e376dc71bafc953c0b0fdeb9983a9956\r
Content-Type: text/plain; charset=utf-8\r
Content-Transfer-Encoding: quoted-printable\r
\r
GZ\r
OoOoO\r
oOoOoOoOo\r
oOoOoOoOoOoOoOoOo\r
oOoOoOoOoOoOoOoOoOoOoOo\r
oOoOoOoOoOoOoOoOoOoOoOoOoOoOo\r
OoOoOoOoOoOoOoOoOoOoOoOoOoOoOoOoO\r
\r
--b1_e376dc71bafc953c0b0fdeb9983a9956\r
Content-Type: text/html; charset=us-ascii\r
\r
<div style=\"text-align: center;\"><strong>GZ</strong><br />\r
OoOoO<br />\r
oOoOoOoOo<br />\r
oOoOoOoOoOoOoOoOo<br />\r
oOoOoOoOoOoOoOoOoOoOoOo<br />\r
oOoOoOoOoOoOoOoOoOoOoOoOoOoOo<br />\r
OoOoOoOoOoOoOoOoOoOoOoOoOoOoOoOoO<br />\r
</div>\r
\r
--b1_e376dc71bafc953c0b0fdeb9983a9956--\r
";

pub static EMAIL2: &[u8] = b"From: alice@example.com\r
To: alice@example.tld\r
Subject: Test\r
\r
Hello world!\r
";
