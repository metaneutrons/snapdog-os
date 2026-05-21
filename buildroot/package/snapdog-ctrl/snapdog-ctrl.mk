################################################################################
#
# snapdog-ctrl
#
# Binary is placed in the rootfs overlay by the build system.
# This package only installs the systemd service.
#
################################################################################

define SNAPDOG_CTRL_INSTALL_INIT_SYSTEMD
	$(INSTALL) -D -m 0644 $(BR2_EXTERNAL_SNAPDOG_PATH)/package/snapdog-ctrl/snapdog-ctrl.service \
		$(TARGET_DIR)/usr/lib/systemd/system/snapdog-ctrl.service
	mkdir -p $(TARGET_DIR)/etc/systemd/system/multi-user.target.wants
	ln -sf /usr/lib/systemd/system/snapdog-ctrl.service \
		$(TARGET_DIR)/etc/systemd/system/multi-user.target.wants/snapdog-ctrl.service
endef

$(eval $(generic-package))
