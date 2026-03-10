export function collectBrowserErrors(page) {
    const consoleErrors = [];
    const pageErrors = [];

    page.on("console", (msg) => {
        if (msg.type() === "error") {
            consoleErrors.push(msg.text());
        }
    });

    page.on("pageerror", (error) => {
        pageErrors.push(error.message);
    });

    return { consoleErrors, pageErrors };
}