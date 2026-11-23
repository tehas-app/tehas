### Changes

- The contract describes 2.x. `host/docs/contract.md` names what is public through every 2.x release, and lists every change 2.0 makes, each with its pull request, its readers and what an adopter does, under "Changed in 2.0". What is still to land before 2.0 is tagged is under "Pending for 2.0".
- The contract names `tehas start`, called by a program for a person signed in to it, as the way a program starts work for a person. In 2.0 it is the one way, once the millwright's grant goes (#2894).
- The update to 2.0 is supported from 1.4.x only, and a host upgrades the consumer that runs many factories, Obeya and its factories together, as the trio.
- `tehasbot refresh --pr N` is no longer public: `tehasbot` is internal, every verb of it. `tehas refresh --pr N` is the command.
- 2.0 rewrites no setting. Each line of an instance is kept, dropped with one warning because its feature left core, or refused with 78; the contract lists which.

### Adopter action

Upgrade a host's consumer that runs many factories, its Obeya and its factories together, from 1.4.x. Before the update, check `instance.env` against "What 2.0 does with a setting" in the contract, fix every line 2.0 refuses, then run `tehas update`, which migrates the state store to schema 28 and analytics to schema 5. A program that reads the factory reads "Changed in 2.0" for each change it is named a reader of. A program that starts work for a person runs `tehas start` for that person, and a script that called `tehasbot refresh --pr N` calls `tehas refresh --pr N`.
