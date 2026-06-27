# WebUI

This WebUI is extracted from SOGo 5. Alinto is working on a from scratch new UI for SOGo 6.
Here are some core component/design of the SOGo 5 UI:
 - Based on Angular JS (1.x). This framework is not supported anymore
 - For the UI, it uses Google Material Design, through a dedicated Angular library
 - HTML is partially rendered by the server through some Objective C code / custom but elegant XML templating
 - But user's data (emails, calendars, etc.) are fetched from a REST API.
 - Internationalization is managed server-side
 - GNUMakefile, Grunt, and npm are used for the local tooling

For the port, we aim at:
 - Keeping Angular 1 for now; if some of the Angular 1 CVE are exploitabale, we'll fix them with a patch of our own.
 - Keeping Material design
 - Drop GNUMakefile, see if we can move to vite or gulp, keep npm and/or go with yarn.
 - Moving to a static frontend (~SPA). No more "SSR" (Server Side Rendering).
   - Templating is done with JSX and rendered ahead of time with @kitajs/html.
   - Internationalization is handled at compile time, we don't know yet how.
   - Some of the server side data injection (eg. current datetime) will be ported to pure browser Javascript code.

The SOGo webUI is only a starting point, in the long term we aim at creating our own webUI by :
 - Gradually replacing Angular 1 with a supported frontend (Angular 2 Typescript is a candidate)
 - Finding compatible libraries with Angular 2 (Material Design will be challenged here)
 - Maybe write our own frontend code (with our own identity + our own low-tech concerns)
 - Maybe rewrite the REST API and/or embed the Aerogramme core as WASM so we get closer to E2EE.
