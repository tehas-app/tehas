### Changes

- The dashboard reads `/host/bridge.json` on its own origin, where a host that fronts the factory's domain can give a link for tickets, notices at the top of the front pages and short notes on tickets. tehas answers 404 under `/host/` itself, so a factory without such a host is unchanged. The dashboard pages unregister any service worker on their origin. `tehas api` lists the feature `host-bridge`. The contract is in the API page, "The host bridge".

### Adopter action

None.
