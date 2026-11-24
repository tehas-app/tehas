### Changes

- The GitHub adapter refuses a text that holds the closing words of one of tehas's markers (`<!-- /tehas:...`) or a marker's words with no `<!--` around them, in five more writes: a blueprint's open, edit and comment, a comment on a pull request, and a commit status's description. Before, these writes refused only an opening marker. The write is refused before anything is sent, as the tickets' notes and edits and the opening of a pull request already did (#3226).
