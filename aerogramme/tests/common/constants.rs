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

pub static ICAL_RFC1: &[u8] = b"BEGIN:VCALENDAR
PRODID:-//Example Corp.//CalDAV Client//EN
VERSION:2.0
BEGIN:VEVENT
UID:1@example.com
SUMMARY:One-off Meeting
DTSTAMP:20041210T183904Z
DTSTART:20041207T120000Z
DTEND:20041207T130000Z
END:VEVENT
BEGIN:VEVENT
UID:2@example.com
SUMMARY:Weekly Meeting
DTSTAMP:20041210T183838Z
DTSTART:20041206T120000Z
DTEND:20041206T130000Z
RRULE:FREQ=WEEKLY
END:VEVENT
BEGIN:VEVENT
UID:2@example.com
SUMMARY:Weekly Meeting
RECURRENCE-ID:20041213T120000Z
DTSTAMP:20041210T183838Z
DTSTART:20041213T130000Z
DTEND:20041213T140000Z
END:VEVENT
END:VCALENDAR
";

pub static ICAL_RFC2: &[u8] = b"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Example Corp.//CalDAV Client//EN
BEGIN:VEVENT
UID:20010712T182145Z-123401@example.com
DTSTAMP:20060712T182145Z
DTSTART:20060714T170000Z
DTEND:20060715T040000Z
SUMMARY:Bastille Day Party
END:VEVENT
END:VCALENDAR
";

pub static ICAL_RFC3: &[u8] = b"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Example Corp.//CalDAV Client//EN
BEGIN:VTIMEZONE
LAST-MODIFIED:20040110T032845Z
TZID:US/Eastern
BEGIN:DAYLIGHT
DTSTART:20000404T020000
RRULE:FREQ=YEARLY;BYDAY=1SU;BYMONTH=4
TZNAME:EDT
TZOFFSETFROM:-0500
TZOFFSETTO:-0400
END:DAYLIGHT
BEGIN:STANDARD
DTSTART:20001026T020000
RRULE:FREQ=YEARLY;BYDAY=-1SU;BYMONTH=10
TZNAME:EST
TZOFFSETFROM:-0400
TZOFFSETTO:-0500
END:STANDARD
END:VTIMEZONE
BEGIN:VEVENT
DTSTART;TZID=US/Eastern:20060104T100000
DURATION:PT1H
SUMMARY:Event #3
UID:DC6C50A017428C5216A2F1CD@example.com
END:VEVENT
END:VCALENDAR
";
