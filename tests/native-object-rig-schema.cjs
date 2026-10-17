'use strict';
/* A minimal validator of the JSON-schema keyword subset the lane-3 payload defs use (not a general validator): type, const,
 * enum, required, properties, additionalProperties:false, items, minItems, maxItems, minLength, maxLength, minimum, maximum,
 * minProperties, oneOf, allOf and local $ref. Used only to show that the closed schema and the contract accept and refuse
 * the same payloads. */
function validator(root) {
  const resolve = (ref) => {
    const name = ref.split('#/$defs/')[1];
    if (!name || !root.$defs[name]) throw new Error('unresolved $ref ' + ref);
    return root.$defs[name];
  };
  const typeOk = (type, v) => ({
    string: typeof v === 'string', boolean: typeof v === 'boolean', number: typeof v === 'number' && Number.isFinite(v),
    integer: typeof v === 'number' && Number.isInteger(v), array: Array.isArray(v), object: v !== null && typeof v === 'object' && !Array.isArray(v),
  })[type];
  function ok(schema, v) {
    if (schema === true) return true;
    if (schema.$ref) return ok(resolve(schema.$ref), v);
    if (schema.allOf && !schema.allOf.every((s) => ok(s, v))) return false;
    if (schema.oneOf && schema.oneOf.filter((s) => ok(s, v)).length !== 1) return false;
    if ('const' in schema && JSON.stringify(schema.const) !== JSON.stringify(v)) return false;
    if (schema.enum && !schema.enum.some((e) => JSON.stringify(e) === JSON.stringify(v))) return false;
    if (schema.type && !typeOk(schema.type, v)) return false;
    if (typeof v === 'string') {
      const length = [...v].length;
      if ((schema.minLength !== undefined && length < schema.minLength) || (schema.maxLength !== undefined && length > schema.maxLength)) return false;
    }
    if (typeof v === 'number') {
      if ((schema.minimum !== undefined && v < schema.minimum) || (schema.maximum !== undefined && v > schema.maximum)) return false;
    }
    if (Array.isArray(v)) {
      if ((schema.minItems !== undefined && v.length < schema.minItems) || (schema.maxItems !== undefined && v.length > schema.maxItems)) return false;
      if (schema.items && !v.every((item) => ok(schema.items, item))) return false;
    }
    if (v !== null && typeof v === 'object' && !Array.isArray(v)) {
      const keys = Object.keys(v), props = schema.properties || {};
      if (schema.required && !schema.required.every((k) => k in v)) return false;
      if (schema.minProperties !== undefined && keys.length < schema.minProperties) return false;
      if (schema.additionalProperties === false && !keys.every((k) => k in props)) return false;
      if (!keys.every((k) => !(k in props) || ok(props[k], v[k]))) return false;
    }
    return true;
  }
  return { ok, def: (name) => root.$defs[name] };
}
module.exports = { validator };
