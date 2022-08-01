# Introduction

<p align="center" style="text-align:center;">
  <img alt="A scan of an Aerogramme dating from 1955" src="./aerogramme.jpg" style="margin:auto; max-width:300px"/>
<br>
[ <strong><a href="https://aerogramme.deuxfleurs.fr/">Documentation</a></strong>
| <a href="https://git.deuxfleurs.fr/Deuxfleurs/aerogramme">Git repository</a>
]
<br>
<em>stability status: technical preview (do not use in production)</em>
</p>

Aerogramme is an open-source **IMAP server** targeted at **distributed** infrastructures and written in **Rust**.
It is designed to be resilient, easy to operate and private by design.

**Resilient** - Aerogramme is built on top of Garage, a (geographically) distributed object storage software. Aerogramme thus inherits Garage resiliency: its mailboxes are spread on multiple distant regions, regions can go offline while keeping mailboxes available, storage nodes can be added or removed on the fly, etc. 

**Easy to operate** - Aerogramme mutualizes the burden of data management by storing all its data in an object store and nothing on the local filesystem or any relational database. It can be seen as a proxy between the IMAP protocol and Garage protocols (S3 and K2V). It can thus be freely moved between machines. Multiple instances can also be run in parallel. 

**Private by design** - As emails are very sensitive, Aerogramme encrypts users' mailboxes with their passwords. Data is decrypted in RAM upon user login: the Garage storage layer handles only encrypted blobs. It is even possible to run locally Aerogramme while connecting it to a remote, third-party, untrusted Garage provider; in this case clear text emails never leak outside of your computer.

Our main use case is to provide a modern email stack for autonomously hosted communities such as [Deuxfleurs](https://deuxfleurs.fr). More generally, we want to set new standards in term of email ethic by lowering the bar to become an email provider while making it harder to spy users' emails.
