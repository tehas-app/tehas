# Checks a JSON value against a JSON Schema, for the keywords the schemas in
# api/ use and no others. A schema with any other keyword is an error, so a
# keyword this does not check is never passed over in silence.
#
#   jq -r --slurpfile schema api/status.schema.json -f test/schema.jq <value>
#
# Prints one line for each way the value breaks the schema; nothing when it
# fits. date-time is the form tehas writes, 2026-10-17T13:59:40Z.

def known: ["$schema", "$id", "title", "description", "type", "const", "enum",
  "required", "additionalProperties", "properties", "items", "minimum", "format"];

def kind: if type == "number" then (if . == floor then "integer" else "number" end) else type end;

def where($p): if $p == "" then "." else $p end;

def check($s; $p):
  . as $v
  | [$s | keys[] | select(IN(known[]) | not) | "\(where($p)): the schema uses \(.), which this check does not know"]
  + (if ($s | has("additionalProperties")) and $s.additionalProperties != false
     then ["\(where($p)): additionalProperties other than false is not checked"] else [] end)
  + (if $s.type == null then []
     else ([$s.type] | flatten) as $types
       | if any($types[]; . == ($v | kind) or (. == "number" and ($v | kind) == "integer")) then []
         else ["\(where($p)): \($v | kind), not \($types | join(" or "))"] end end)
  + (if ($s | has("const")) and $v != $s.const then ["\(where($p)): \($v | tojson), not \($s.const | tojson)"] else [] end)
  + (if $s.enum == null or any($s.enum[]; . == $v) then []
     else ["\(where($p)): \($v | tojson) is not one of \($s.enum | tojson)"] end)
  + (if ($v | type) == "number" and $s.minimum != null and $v < $s.minimum
     then ["\(where($p)): \($v) is under \($s.minimum)"] else [] end)
  + (if $s.format == null then []
     elif $s.format != "date-time" then ["\(where($p)): the format \($s.format) is not checked"]
     elif ($v | type) != "string" or ($v | try (fromdateiso8601 | true) catch false) then []
     else ["\(where($p)): \($v | tojson) is not a date-time"] end)
  + (if ($v | type) != "object" then []
     else ($s.properties // {}) as $props
       | [($s.required // [])[] | select(. as $k | $v | has($k) | not) | "\(where($p)): \(.) is missing"]
       + (if $s.additionalProperties == false
          then [$v | keys_unsorted[] | select(. as $k | $props | has($k) | not) | "\($p).\(.): not in the schema"]
          else [] end)
       + ([$v | to_entries[] | select(.key as $k | $props | has($k))
           | .key as $k | .value | check($props[$k]; "\($p).\($k)")] | add // [])
     end)
  + (if ($v | type) != "array" or $s.items == null then []
     else [range($v | length) as $i | $v[$i] | check($s.items; "\($p)[\($i)]")] | add // [] end);

check($schema[0]; "")[]
