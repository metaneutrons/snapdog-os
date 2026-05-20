################################################################################
#
# snapdog-ctrl
#
################################################################################

SNAPDOG_CTRL_VERSION = 0.1.0
SNAPDOG_CTRL_SOURCE = snapdog-ctrl-$(SNAPDOG_CTRL_VERSION)-aarch64-unknown-linux-gnu.tar.gz
SNAPDOG_CTRL_SITE = https://github.com/metaneutrons/snapdog-ctrl/releases/download/v$(SNAPDOG_CTRL_VERSION)
SNAPDOG_CTRL_LICENSE = GPL-3.0-only

define SNAPDOG_CTRL_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/snapdog-ctrl \
		$(TARGET_DIR)/usr/bin/snapdog-ctrl
endef

define SNAPDOG_CTRL_INSTALL_INIT_SYSTEMD
	$(INSTALL) -D -m 0644 $(BR2_EXTERNAL_SNAPDOG_PATH)/package/snapdog-ctrl/snapdog-ctrl.service \
		$(TARGET_DIR)/usr/lib/systemd/system/snapdog-ctrl.service
	mkdir -p $(TARGET_DIR)/etc/systemd/system/multi-user.target.wants
	ln -sf /usr/lib/systemd/system/snapdog-ctrl.service \
		$(TARGET_DIR)/etc/systemd/system/multi-user.target.wants/snapdog-ctrl.service
endef

$(eval $(generic-package))
