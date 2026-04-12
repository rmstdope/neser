import { Page, ConsoleMessage } from "@playwright/test";

export function collectBrowserErrors(page: Page) {
    const consoleErrors: string[] = [];
    const pageErrors: string[] = [];

    page.on("console", (msg: ConsoleMessage) => {
        if (msg.type() === "error") {
            consoleErrors.push(msg.text());
        }
    });

    page.on("pageerror", (error: Error) => {
        pageErrors.push(error.message);
    });

    return { consoleErrors, pageErrors };
}