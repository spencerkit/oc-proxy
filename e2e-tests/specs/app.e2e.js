describe("AI Open Router", () => {
  const groupId = "e2e"
  const groupName = "E2E Group"
  const providerName = "E2E Provider"
  const providerModel = "gpt-4o-mini"

  const selectors = {
    errorScreen: ".error-screen",
    firstRunTitle: '//h2[contains(., "Start by creating your first group")]',
    groupInfoTitle: '//h3[contains(., "Group Info")]',
    addGroupButton: '//button[@aria-label="Add Group" or @title="Add Group"]',
    createFirstGroupButton: '//button[contains(., "Create First Group")]',
    createModalButton: '//button[normalize-space()="Create"]',
    providersNav: '//button[.//span[normalize-space()="Providers"]]',
    serviceNav: '//button[.//span[normalize-space()="Service"]]',
    logsNav: '//button[.//span[normalize-space()="Logs"]]',
    addProviderButton: '//button[normalize-space()="Add Provider"]',
    createProviderButton: '//button[normalize-space()="Create Provider"]',
    associateProviderButton: '//button[@title="Associate Provider"]',
    logsTitle: '//h2[normalize-space()="Logs"]',
    startButton: '//button[normalize-space()="Start"]',
    stopButton: '//button[normalize-space()="Stop"]',
  }

  const groupPathSelector = id => `//span[normalize-space()="/${id}"]`
  const groupButtonSelector = id => `//button[.//span[normalize-space()="/${id}"]]`
  const providerNameSelector = name => `//span[normalize-space()="${name}"]`

  async function waitForNoModal() {
    await browser.waitUntil(
      async () => {
        const dialogs = await $$('div[role="dialog"]')
        return dialogs.length === 0
      },
      { timeout: 10000, timeoutMsg: "Modal did not close in time" }
    )
  }

  async function safeClick(selector) {
    const el = await $(selector)
    await el.waitForExist({ timeout: 10000 })
    try {
      await el.click()
    } catch {
      await browser.execute(target => target.click(), el)
    }
  }

  async function waitForReady() {
    await browser.waitUntil(
      async () => {
        const errorScreen = await $(selectors.errorScreen)
        if (await errorScreen.isExisting()) return true
        const firstRunTitle = await $(selectors.firstRunTitle)
        if (await firstRunTitle.isExisting()) return true
        const groupInfo = await $(selectors.groupInfoTitle)
        return groupInfo.isExisting()
      },
      {
        timeout: 30000,
        timeoutMsg: "App did not reach a ready state within 30s",
      }
    )

    const errorScreen = await $(selectors.errorScreen)
    if (await errorScreen.isExisting()) {
      const message = await errorScreen.getText()
      throw new Error(`App bootstrap failed: ${message || "unknown error"}`)
    }
  }

  async function ensureEnglish() {
    const enButton = await $('//button[normalize-space()="EN"]')
    if (await enButton.isExisting()) {
      await enButton.click()
    }
  }

  async function ensureGroup() {
    const groupPath = await $(groupPathSelector(groupId))
    if (!(await groupPath.isExisting())) {
      const createFirst = await $(selectors.createFirstGroupButton)
      if (await createFirst.isExisting()) {
        await safeClick(selectors.createFirstGroupButton)
      } else {
        await safeClick(selectors.addGroupButton)
      }

      const groupIdInput = await $("#groupId")
      await groupIdInput.waitForExist({ timeout: 10000 })
      await groupIdInput.setValue(groupId)
      await $("#groupName").setValue(groupName)

      await safeClick(selectors.createModalButton)
      await $(groupPathSelector(groupId)).waitForExist({ timeout: 15000 })
    }

    await safeClick(groupButtonSelector(groupId))
  }

  async function ensureProviderExists() {
    await safeClick(selectors.providersNav)
    const providersTitle = await $('//h2[normalize-space()="Providers"]')
    await providersTitle.waitForExist({ timeout: 15000 })

    const providerNameEl = await $(providerNameSelector(providerName))
    if (!(await providerNameEl.isExisting())) {
      await safeClick(selectors.addProviderButton)

      await $("#name").waitForExist({ timeout: 15000 })
      await $("#name").setValue(providerName)
      await $("#defaultModel").setValue(providerModel)
      await $("#token").setValue("sk-e2e")
      await $("#apiAddress").setValue("https://api.openai.com/v1")

      const openaiButton = await $('//button[normalize-space()="OpenAI"]')
      if (await openaiButton.isExisting()) {
        await safeClick('//button[normalize-space()="OpenAI"]')
      }

      await safeClick(selectors.createProviderButton)

      await providersTitle.waitForExist({ timeout: 15000 })
    }
  }

  async function ensureProviderAssociated() {
    await safeClick(selectors.serviceNav)
    await $(selectors.groupInfoTitle).waitForExist({ timeout: 15000 })

    const providerCard = await $(providerNameSelector(providerName))
    if (await providerCard.isExisting()) return

    await safeClick(selectors.associateProviderButton)

    await safeClick(`//label[.//span[normalize-space()="${providerName}"]]`)

    await safeClick('//button[normalize-space()="Associate Provider"]')
    await waitForNoModal()

    await $(providerNameSelector(providerName)).waitForExist({ timeout: 15000 })
  }

  async function tryToggleService() {
    const stopButton = await $(selectors.stopButton)
    if (await stopButton.isExisting()) {
      await safeClick(selectors.stopButton)
      await $(selectors.startButton).waitForExist({ timeout: 20000 })
    }

    const startButton = await $(selectors.startButton)
    if (await startButton.isExisting()) {
      await safeClick(selectors.startButton)
      await browser.waitUntil(
        async () => {
          const stopNow = await $(selectors.stopButton)
          if (await stopNow.isExisting()) return true
          const errorToast = await $(
            '//div[@role="alert"]//span[contains(., "Start failed: port is already in use")]'
          )
          return errorToast.isExisting()
        },
        { timeout: 20000, timeoutMsg: "Service did not start or report a known error" }
      )
    }
  }

  it("covers the main flow", async () => {
    await waitForReady()

    const title = await browser.getTitle()
    expect(title).toBe("AI Open Router")

    await ensureEnglish()
    await ensureGroup()
    await ensureProviderExists()
    await ensureProviderAssociated()
    await tryToggleService()

    await waitForNoModal()
    await safeClick(selectors.logsNav)
    await $(selectors.logsTitle).waitForExist({ timeout: 15000 })
  })
})
