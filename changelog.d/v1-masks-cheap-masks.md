### Changes

- A plan's title and body, the comment that ends a planning session and the comment on an issue a refine replaced are masked before they reach GitHub, as other planner text already was. A plan's title is cut at 256 characters.
- The crates' build scripts in `install.sh` are no longer given the model keys or any other variable whose name holds KEY, PASSWD or CREDENTIAL, in any case.
