'use strict';
// #2113 (G06 G17): bitmap-brush publication cancelled before dispatch, and the shared image quota exhausted by 64 MiB bitmaps.
// Contract: native-bitmap-brush-resources.json ("A publication changes no document revision and adds no history entry ... an exhausted quota
// [is a] typed refusal that leaves the store and the quota unchanged"), the quota being the shared image budget (native-engine/src/image_resource_budget.rs:
// MAX_BYTES 384 MiB), and the request-level cancellation of the v2 envelope (nemo-mcp contract.rs cancelled_before_dispatch).
const P = require('../payloads.cjs');
const K = require('../kit.cjs');

const BIG = 4096, MAX_BIG_PUBLICATIONS = 7; // 7 x 64 MiB = 448 MiB > the 384 MiB quota: a refusal must come first
const attach = (ctx, label, file, name) => K.apply(ctx, label, () => K.pinned(ctx, ({ pin, lifecycleGeneration }) => P.assetAttach({ pin, name, path: file, lifecycleGeneration })));
const publication = (pin, assetId, brushId, size) => P.brushPublish(pin, assetId, brushId, size, size);

const rows = [
  { id: '2113-bitmap-brush-quota-and-cancel', title: 'bitmap brush: a publication cancelled before dispatch changes nothing and can be sent again; the shared quota refuses a 64 MiB bitmap without changing the store, and replacing the large bitmaps frees it', tasks: ['G06', 'G17'],
    limitations: ['The quota is the process-wide shared image budget: the row stops at the first refusal and gives the quota back by replacing the large bitmaps with a small one.'],
    async run(ctx) {
      await K.fresh(ctx, { transparent: true });
      const inspect = async brushId => {
        const { revision, pin } = await K.pinNow(ctx), b = P.brushInspect(revision, pin, brushId), res = await ctx.wire.query(b.operation, b.payload);
        ctx.ok('reply', 'brush inspect answers for ' + brushId, res.ok, res.ok ? 'ok' : res.error);
        return res.result.brush;
      };
      const publish = (label, assetId, brushId, size, opts) => K.resource(ctx, label, () => K.pinned(ctx, ({ pin }) => publication(pin, assetId, brushId, size)), opts);
      const tip = await attach(ctx, 'attach the 8x8 tip image', K.stage(ctx, 'tip8.png', K.solidPng(8, 8, [10, 10, 10, 255])), 'tip');
      // ---- cancel: the owner answers a typed cancellation, publishes nothing, and the same publication is accepted afterwards ----
      const cancelled = await publish('publish cancelled before dispatch', tip.result.assetId, 'cancel-brush', 8, { refused: true, codes: ['cancelled_before_dispatch'], send: { cancelledBeforeDispatch: true }, details: error => !!error.details && error.details.reason === 'cancelled' });
      ctx.eq('read', 'a cancelled publication leaves no brush behind', (await inspect('cancel-brush')).resident, false);
      ctx.ok('reply', 'the cancellation is the owner\'s typed answer, not an acceptance', cancelled.res.ok === false && cancelled.res.error.code === 'cancelled_before_dispatch', cancelled.res.ok ? 'accepted' : cancelled.res.error.code);
      const again = await publish('send the same publication again', tip.result.assetId, 'cancel-brush', 8, { applied: true });
      const row = await inspect('cancel-brush');
      ctx.eq('read', 'the brush is resident with the size and bytes the reply reports (no charge or mark was left by the cancellation)', [row.resident, row.width, row.height, row.bytes, row.version], [true, 8, 8, again.result.bytes, again.result.version]);
      // ---- quota: distinct 4096x4096 bitmaps until the shared 384 MiB quota refuses one ----
      const resident = [];
      let refused = null;
      for (let i = 1; i <= MAX_BIG_PUBLICATIONS && refused === null; i++) {
        const asset = await attach(ctx, 'attach the large image ' + i, K.stage(ctx, 'big' + i + '.png', K.solidPng(BIG, BIG, [(i * 31) % 256, 30, 40, 255])), 'big' + i);
        const brushId = 'quota-' + i;
        const res = await K.pinned(ctx, async ({ pin }) => { const b = publication(pin, asset.result.assetId, brushId, BIG); return ctx.wire.command(b.operation, b.payload); });
        if (res.ok) resident.push({ brushId, assetId: asset.result.assetId });
        else refused = { brushId, assetId: asset.result.assetId, first: res.error };
      }
      ctx.ok('reply', 'the shared quota refuses a publication within ' + MAX_BIG_PUBLICATIONS + ' bitmaps of 64 MiB (their sum exceeds 384 MiB)', refused !== null, refused ? refused.brushId : 'no refusal');
      ctx.ok('reply', 'at least one large bitmap was accepted before the refusal (the refusal is about the quota, not the size)', resident.length >= 1, resident.length, '>=1');
      const held = await Promise.all(resident.map(r => inspect(r.brushId)));
      ctx.eq('read', 'every accepted large bitmap is resident at 64 MiB', held.map(h => [h.resident, h.width, h.height, h.bytes]), resident.map(() => [true, BIG, BIG, BIG * BIG * 4]));
      // The refusal again, now bracketed by the kit: typed owner `unavailable` with reason over-quota, no revision, no history entry.
      await publish('publish the refused bitmap again', refused.assetId, refused.brushId, BIG, { refused: true, codes: ['unavailable'], details: error => !!error.details && error.details.reason === 'over-quota' });
      ctx.eq('read', 'the refused brush is not resident', (await inspect(refused.brushId)).resident, false);
      ctx.eq('read', 'the store is as it was: every earlier bitmap has the same version and bytes', await Promise.all(resident.map(r => inspect(r.brushId))), held);
      // ---- the quota comes back when the large bitmaps are replaced by a small one (the replaced storage is freed) ----
      for (const r of resident) await publish('replace ' + r.brushId + ' by the 8x8 tip', tip.result.assetId, r.brushId, 8, { applied: true });
      ctx.eq('read', 'every replaced brush is the small bitmap', (await Promise.all(resident.map(r => inspect(r.brushId)))).every(b => b.resident && b.width === 8 && b.bytes < 4096), true);
      const accepted = await publish('publish the previously refused bitmap', refused.assetId, refused.brushId, BIG, { applied: true });
      const now = await inspect(refused.brushId);
      ctx.eq('read', 'it is now resident at 64 MiB', [now.resident, now.bytes], [true, accepted.result.bytes]);
      // Give the quota back for the rows that follow.
      await publish('replace ' + refused.brushId + ' by the 8x8 tip', tip.result.assetId, refused.brushId, 8, { applied: true });
      ctx.eq('read', 'the quota is given back: the last large bitmap is small again', (await inspect(refused.brushId)).bytes < 4096, true);
    } },
];

module.exports = { rows };
