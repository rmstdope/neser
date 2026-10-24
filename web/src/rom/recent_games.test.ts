/**
 * @vitest-environment jsdom
 */
import "fake-indexeddb/auto";

import { describe, expect, it, vi } from "vitest";

import {
    applyPendingThumbnail,
    createRecentGamesStore,
    keepThumbnail,
    PENDING_THUMBNAIL_KEY,
    playedWhen,
    RECENT_GAMES_LIMIT,
    renderRecentGames,
    type RecentGame,
    type RecentGamesStore,
} from "./recent_games";

const game = (name: string, playedAt: number, extra: Partial<RecentGame & { bytes: Uint8Array }> = {}): RecentGame & { bytes: Uint8Array } => ({
    name,
    system: "NES",
    bytes: new Uint8Array([1, 2, 3]),
    fromFile: true,
    playedAt,
    thumbnail: null,
    ...extra,
});

let dbCount = 0;
const freshStore = () => createRecentGamesStore(`neser-recent-test-${dbCount++}`);

describe("the recent games store", () => {
    it("Given games played, When listed, Then the latest played comes first", async () => {
        const store = freshStore();
        await store.remember(game("a.nes", 1));
        await store.remember(game("b.gb", 3));
        await store.remember(game("c.sfc", 2));
        expect((await store.list()).map((g) => g.name)).toEqual(["b.gb", "c.sfc", "a.nes"]);
    });

    it("Given a game played again, When remembered, Then it is listed once, at the top, keeping its picture", async () => {
        const store = freshStore();
        await store.remember(game("a.nes", 1));
        await store.setThumbnail("a.nes", "data:image/png;base64,AAA");
        await store.remember(game("b.gb", 2));
        await store.remember(game("a.nes", 3));
        const list = await store.list();
        expect(list.map((g) => g.name)).toEqual(["a.nes", "b.gb"]);
        expect(list[0].thumbnail).toBe("data:image/png;base64,AAA");
    });

    it("Given more games than the limit, When remembered, Then the least recently played are forgotten", async () => {
        const store = freshStore();
        for (let i = 0; i < RECENT_GAMES_LIMIT + 2; i++) await store.remember(game(`g${i}.nes`, i));
        const names = (await store.list()).map((g) => g.name);
        expect(names).toHaveLength(RECENT_GAMES_LIMIT);
        expect(names).not.toContain("g0.nes");
        expect(names).not.toContain("g1.nes");
        expect(names[0]).toBe(`g${RECENT_GAMES_LIMIT + 1}.nes`);
    });

    it("Given remembered games, When listed, Then only what the cards need comes back, and a game's bytes only when asked for", async () => {
        const store = freshStore();
        await store.remember(game("a.nes", 1, { bytes: new Uint8Array([9, 8, 7]) }));
        const [listed] = await store.list();
        expect("bytes" in listed).toBe(false);
        expect([...(await store.bytesOf("a.nes"))!]).toEqual([9, 8, 7]);
        expect(await store.bytesOf("never.nes")).toBeNull();
    });

    it("Given remembered games, When cleared, Then neither the cards nor the bytes are left", async () => {
        const store = freshStore();
        await store.remember(game("a.nes", 1));
        await store.clear();
        expect(await store.list()).toEqual([]);
        expect(await store.bytesOf("a.nes")).toBeNull();
    });

    it("Given a game forgotten beyond the limit, When its bytes are asked for, Then they are gone too", async () => {
        const store = freshStore();
        for (let i = 0; i < RECENT_GAMES_LIMIT + 1; i++) await store.remember(game(`g${i}.nes`, i));
        expect(await store.bytesOf("g0.nes")).toBeNull();
        expect(await store.bytesOf(`g${RECENT_GAMES_LIMIT}.nes`)).not.toBeNull();
    });

    it("Given a picture for a game no longer remembered, When set, Then nothing is stored", async () => {
        const store = freshStore();
        await store.setThumbnail("gone.nes", "data:x");
        expect(await store.list()).toEqual([]);
    });
});

describe("a database from before the bytes had their own store", () => {
    it("Given the old one-store database, When the store opens it, Then it upgrades and remembers games again", async () => {
        const name = `neser-recent-old-${dbCount++}`;
        await new Promise<void>((resolve, reject) => {
            const request = indexedDB.open(name, 1);
            request.onupgradeneeded = () => request.result.createObjectStore("games", { keyPath: "name" });
            request.onsuccess = () => {
                request.result.close();
                resolve();
            };
            request.onerror = () => reject(request.error);
        });
        const store = createRecentGamesStore(name);
        await store.remember(game("a.nes", 1));
        expect((await store.list()).map((g) => g.name)).toEqual(["a.nes"]);
        expect(await store.bytesOf("a.nes")).not.toBeNull();
    });
});

describe("a picture taken as the page is left", () => {
    const memoryStorage = () => {
        const items = new Map<string, string>();
        return {
            getItem: (key: string) => items.get(key) ?? null,
            setItem: (key: string, value: string) => void items.set(key, value),
            removeItem: (key: string) => void items.delete(key),
        };
    };
    // The page went away before the write committed: Chromium aborts it, so it never lands.
    const abortedWrites = (store: RecentGamesStore): RecentGamesStore => ({ ...store, setThumbnail: () => new Promise<void>(() => {}) });

    it("Given its database write was lost with the page, When the next page starts, Then the card has the picture", async () => {
        const store = freshStore();
        await store.remember(game("a.nes", 1));
        const storage = memoryStorage();

        void keepThumbnail(abortedWrites(store), storage, "a.nes", "data:image/png;base64,AAA");
        await applyPendingThumbnail(store, storage);

        expect((await store.list())[0].thumbnail).toBe("data:image/png;base64,AAA");
        expect(storage.getItem(PENDING_THUMBNAIL_KEY)).toBeNull();
    });

    it("Given its database write committed, Then nothing is left waiting for the next page", async () => {
        const store = freshStore();
        await store.remember(game("a.nes", 1));
        const storage = memoryStorage();

        await keepThumbnail(store, storage, "a.nes", "data:image/png;base64,AAA");

        expect((await store.list())[0].thumbnail).toBe("data:image/png;base64,AAA");
        expect(storage.getItem(PENDING_THUMBNAIL_KEY)).toBeNull();
    });

    it("Given a later picture taken while the first was still being written, Then the later one is what waits", async () => {
        const store = freshStore();
        await store.remember(game("a.nes", 1));
        const storage = memoryStorage();

        const first = keepThumbnail(store, storage, "a.nes", "data:image/png;base64,AAA");
        void keepThumbnail(abortedWrites(store), storage, "a.nes", "data:image/png;base64,BBB");
        await first;
        await applyPendingThumbnail(store, storage);

        expect((await store.list())[0].thumbnail).toBe("data:image/png;base64,BBB");
    });

    it("Given no storage the page may use, Then the picture is still written to the database", async () => {
        const store = freshStore();
        await store.remember(game("a.nes", 1));

        await keepThumbnail(store, null, "a.nes", "data:image/png;base64,AAA");
        await applyPendingThumbnail(store, null);

        expect((await store.list())[0].thumbnail).toBe("data:image/png;base64,AAA");
    });
});

describe("playedWhen", () => {
    const now = new Date(2026, 9, 23, 15, 0);
    it("Given when a game was played, When said in words, Then it reads Today, Yesterday, days, Last week, then the date", () => {
        expect(playedWhen(now, new Date(2026, 9, 23, 1, 0))).toBe("Today");
        expect(playedWhen(now, new Date(2026, 9, 22, 23, 0))).toBe("Yesterday");
        expect(playedWhen(now, new Date(2026, 9, 20, 12, 0))).toBe("3 days ago");
        expect(playedWhen(now, new Date(2026, 9, 14, 12, 0))).toBe("Last week");
        expect(playedWhen(now, new Date(2026, 8, 1, 12, 0))).toBe(new Date(2026, 8, 1).toLocaleDateString());
    });
});

describe("renderRecentGames", () => {
    const now = new Date(2026, 9, 23, 15, 0);

    it("Given recent games, When drawn, Then each is a card with its picture or system, its name and when it was played, and pressing it plays it", () => {
        const el = document.createElement("div");
        const onPick = vi.fn();
        renderRecentGames(el, [game("roms/tetris.gb", now.getTime(), { system: "GB", thumbnail: "data:image/png;base64,AAA" }), game("star.sfc", now.getTime() - 86_400_000, { system: "SNES" })], now, onPick);
        const cards = [...el.querySelectorAll<HTMLButtonElement>("button.recent-card")];
        expect(cards.map((c) => c.querySelector(".recent-card-name")!.textContent)).toEqual(["tetris", "star"]);
        expect(cards.map((c) => c.querySelector(".recent-card-when")!.textContent)).toEqual(["Today", "Yesterday"]);
        expect(cards[0].querySelector("img")!.getAttribute("src")).toBe("data:image/png;base64,AAA");
        expect(cards[1].querySelector("img")).toBeNull();
        expect(cards[1].querySelector(".sys-badge")!.textContent).toBe("SNES");
        expect(cards[0].getAttribute("aria-label")).toBe("Play tetris again");
        cards[1].click();
        expect(onPick).toHaveBeenCalledWith(expect.objectContaining({ name: "star.sfc" }));
    });

    it("Given no recent games, When drawn, Then the row is empty", () => {
        const el = document.createElement("div");
        renderRecentGames(el, [], now, () => {});
        expect(el.children).toHaveLength(0);
    });
});
