![Aerogramme logo](https://aerogramme.deuxfleurs.fr/logo/aerogramme-blue-hz.svg)

# Aerogramme - Encrypted e-mail storage over Garage

⚠️ **TECHNOLOGICAL PREVIEW, THIS SERVER IS NOT READY FOR PRODUCTION OR EVEN BETA TESTING**

A resilient & standards-compliant open-source IMAP server with built-in encryption 

## Quickly jump to our website!

<a href="https://aerogramme.deuxfleurs.fr/download/"><img height="100" src="https://aerogramme.deuxfleurs.fr/images/download.png" alt="Download"/></a>
<a href="https://aerogramme.deuxfleurs.fr/documentation/quick-start/"><img height="100" src="https://aerogramme.deuxfleurs.fr/images/getting-started.png" alt="Getting Start"/></a>

[RFC Coverage](https://aerogramme.deuxfleurs.fr/documentation/reference/rfc/) -
[Design overview](https://aerogramme.deuxfleurs.fr/documentation/design/overview/) -
[Mailbox Datastructure](https://aerogramme.deuxfleurs.fr/documentation/design/mailbox/) -
[Mailbox Mutation Log](https://aerogramme.deuxfleurs.fr/documentation/design/log/).

## Roadmap

  - ✅ 0.1 Better emails parsing.
  - ✅ 0.2 Support of IMAP4..
  - ✅ 0.3 CalDAV support.
  - ⌛0.4 CardDAV support.
  - ⌛0.5 Public beta.

## A note about cargo2nix

Currently, you must edit Cargo.nix by hand after running `cargo2nix`.
Find the `tokio` dependency declaration. 
Look at tokio's dependencies, the `tracing` is disable through a `if false` logic.
Activate it by replacing the condition with `if true`.


## Sponsors and funding

[Aerogramme project](https://nlnet.nl/project/Aerogramme/) is funded through the NGI Assure Fund, a fund established by NLnet with financial support from the European Commission's Next Generation Internet programme, under the aegis of DG Communications Networks, Content and Technology under grant agreement No 957073.

![NLnet logo](https://aerogramme.deuxfleurs.fr/images/nlnet.svg)

## License

EUROPEAN UNION PUBLIC LICENCE v. 1.2
EUPL © the European Union 2007, 2016

This European Union Public Licence (the ‘EUPL’) applies to the Work (as defined
below) which is provided under the terms of this Licence. Any use of the Work,
other than as authorised under this Licence is prohibited (to the extent such
use is covered by a right of the copyright holder of the Work).

The Work is provided under the terms of this Licence when the Licensor (as
defined below) has placed the following notice immediately following the
copyright notice for the Work:

Licensed under the EUPL

or has expressed by any other means his willingness to license under the EUPL.
