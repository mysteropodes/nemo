'use strict';
// A mini JSON Schema (draft 2020-12 subset) validator for the composed L09-K tests (not a test file). It implements exactly
// the keywords native-transport-v2.schema.json uses for the export definitions: $ref (local #/$defs), type, const, enum,
// required, properties, additionalProperties:false, items, minItems, maxItems, minimum, maximum, pattern, oneOf, anyOf,
// allOf, not. An unsupported keyword is an error, so a schema edit cannot silently weaken the check.
const SUPPORTED = new Set(['$ref', 'type', 'const', 'enum', 'required', 'properties', 'additionalProperties', 'items', 'minItems', 'maxItems', 'minimum', 'maximum', 'pattern', 'oneOf', 'anyOf', 'allOf', 'not', 'description', 'title', '$schema', '$id', 'examples', 'minLength']);
const typeOf = (value) => (value === null ? 'null' : Array.isArray(value) ? 'array' : Number.isInteger(value) ? 'integer' : typeof value);
function validator(root) {
  const resolve = (ref) => {
    if (!ref.startsWith('#/')) throw new Error('unsupported $ref ' + ref);
    return ref.slice(2).split('/').reduce((node, key) => node[key], root);
  };
  function check(schema, value) {
    for (const key of Object.keys(schema)) if (!SUPPORTED.has(key)) throw new Error('unsupported keyword ' + key);
    if (schema.$ref !== undefined && !check(resolve(schema.$ref), value)) return false;
    if (schema.type !== undefined) {
      const kinds = [].concat(schema.type);
      const actual = typeOf(value);
      if (!kinds.some((k) => k === actual || (k === 'number' && actual === 'integer'))) return false;
    }
    if (schema.const !== undefined && JSON.stringify(schema.const) !== JSON.stringify(value)) return false;
    if (schema.enum !== undefined && !schema.enum.some((e) => JSON.stringify(e) === JSON.stringify(value))) return false;
    if (typeof value === 'number') {
      if (schema.minimum !== undefined && value < schema.minimum) return false;
      if (schema.maximum !== undefined && value > schema.maximum) return false;
    }
    if (typeof value === 'string') {
      if (schema.pattern !== undefined && !new RegExp(schema.pattern).test(value)) return false;
      if (schema.minLength !== undefined && value.length < schema.minLength) return false;
    }
    if (Array.isArray(value)) {
      if (schema.minItems !== undefined && value.length < schema.minItems) return false;
      if (schema.maxItems !== undefined && value.length > schema.maxItems) return false;
      if (schema.items !== undefined && !value.every((item) => check(schema.items, item))) return false;
    }
    if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
      if (schema.required !== undefined && !schema.required.every((k) => Object.prototype.hasOwnProperty.call(value, k))) return false;
      const props = schema.properties || {};
      for (const key of Object.keys(value)) {
        if (Object.prototype.hasOwnProperty.call(props, key)) { if (!check(props[key], value[key])) return false; } else if (schema.additionalProperties === false) return false;
      }
    }
    if (schema.allOf !== undefined && !schema.allOf.every((s) => check(s, value))) return false;
    if (schema.anyOf !== undefined && !schema.anyOf.some((s) => check(s, value))) return false;
    if (schema.oneOf !== undefined && schema.oneOf.filter((s) => check(s, value)).length !== 1) return false;
    if (schema.not !== undefined && check(schema.not, value)) return false;
    return true;
  }
  return { check, def: (name) => (value) => check({ $ref: '#/$defs/' + name }, value) };
}
module.exports = { validator };
