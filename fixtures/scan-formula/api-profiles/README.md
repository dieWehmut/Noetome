# Supplemental API configuration freeze

`authorized-stop.json` is a public configuration for the sole selected live
route. It reserves zero requests and a token cap of one, so it must stop before
any network request. It contains an environment-variable reference, no secret.
It is not a snapshot of current account usage, remaining authorization or an
approved live-call balance.

`frozen-sha256.json` pins this supplemental configuration separately. The author
formula's original 33-file `../frozen-sha256.json` and all frozen image/source/
expected bytes remain unchanged. Artificial numeric-loopback configurations
are generated only in new independent audit roots and are labeled synthetic.
Actual live settings must be prepared from explicitly restored current usage
and credential evidence; this directory does not restore those external files.
