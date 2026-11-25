### Changes

- The plugins' gate and `tehas plugin call --login` ask GitHub through the GitHub adapter, and no longer read `TEHAS_GITHUB_API`: they always ask `https://api.github.com`, with every token and secret on curl's standard input.
