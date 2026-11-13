// urlbox 的 macOS 定位桥。
//
// 背景：WKWebView 在 macOS 上并没有暴露 WKUIDelegate 的定位授权回调，
// wry 也没实现，于是 navigator.geolocation 一律返回 "User denied Geolocation"。
// 这里用原生 CoreLocation 补齐：
//   1. 首次请求触发系统定位授权（一次性，系统强制）；
//   2. 授权后取一次坐标，回传给 Rust，再由 Rust 注入到网页里。

#import <Foundation/Foundation.h>
#import <CoreLocation/CoreLocation.h>

static void UBGeoLog(NSString *fmt, ...) {
    if (!getenv("URLBOX_TRACE")) return;
    va_list ap; va_start(ap, fmt);
    NSLogv(fmt, ap);
    va_end(ap);
}

typedef void (*UBGeoCallback)(int ok, double lat, double lng, double acc,
                             const char *err, void *ctx);

@interface UBGeoDelegate : NSObject <CLLocationManagerDelegate>
@property(nonatomic, strong) CLLocationManager *manager;
@property(nonatomic, assign) UBGeoCallback callback;
@property(nonatomic, assign) void *ctx;
@property(nonatomic, assign) BOOL started;
@end

static UBGeoDelegate *gDelegate = nil;

@implementation UBGeoDelegate

- (void)finishWithOk:(BOOL)ok
                  lat:(double)lat
                  lng:(double)lng
                  acc:(double)acc
                  err:(NSString *)err {
    if (!self.callback) {
        return;
    }
    const char *msg = err ? [err UTF8String] : NULL;
    UBGeoCallback cb = self.callback;
    void *ctx = self.ctx;
    // 一次性回调：回调前清空，避免重复触发
    self.callback = NULL;
    self.ctx = NULL;
    cb(ok ? 1 : 0, lat, lng, acc, msg, ctx);
    [self.manager stopUpdatingLocation];
}

- (void)locationManagerDidChangeAuthorization:(CLLocationManager *)manager {
    CLAuthorizationStatus status = manager.authorizationStatus;
    UBGeoLog(@"[urlbox-geo] didChangeAuthorization status=%ld", (long)status);
    // macOS 只有 kCLAuthorizationStatusAuthorized（WhenInUse 仅 iOS 可用）
    if (status == kCLAuthorizationStatusAuthorized) {
        if (!self.started) {
            self.started = YES;
            [manager requestLocation];
        }
    } else if (status == kCLAuthorizationStatusDenied) {
        [self finishWithOk:NO lat:0 lng:0 acc:0 err:@"定位权限被拒绝（系统设置 → 隐私与安全 → 定位服务）"];
    } else if (status == kCLAuthorizationStatusRestricted) {
        [self finishWithOk:NO lat:0 lng:0 acc:0 err:@"定位被系统策略限制"];
    }
}

- (void)locationManager:(CLLocationManager *)manager
     didUpdateLocations:(NSArray<CLLocation *> *)locations {
    CLLocation *loc = locations.lastObject;
    if (!loc) {
        [self finishWithOk:NO lat:0 lng:0 acc:0 err:@"未取到有效坐标"];
        return;
    }
    CLLocationCoordinate2D c = loc.coordinate;
    [self finishWithOk:YES
                    lat:c.latitude
                    lng:c.longitude
                    acc:loc.horizontalAccuracy
                    err:nil];
}

- (void)locationManager:(CLLocationManager *)manager
       didFailWithError:(NSError *)error {
    UBGeoLog(@"[urlbox-geo] didFail %@", error.localizedDescription);
    [self finishWithOk:NO
                    lat:0
                    lng:0
                    acc:0
                    err:error.localizedDescription ?: @"定位失败"];
}

@end

/// 触发一次定位：未授权时会先弹出系统授权框（仅首次）。
/// 回调在主线程执行，且只会被调用一次。
static void UBGeoPerformRequest(UBGeoCallback callback, void *ctx) {
    {
        if (!gDelegate) {
            gDelegate = [[UBGeoDelegate alloc] init];
            gDelegate.manager = [[CLLocationManager alloc] init];
            gDelegate.manager.delegate = gDelegate;
            gDelegate.manager.desiredAccuracy = kCLLocationAccuracyHundredMeters;
        }
        gDelegate.callback = callback;
        gDelegate.ctx = ctx;
        gDelegate.started = NO;

        CLAuthorizationStatus status = gDelegate.manager.authorizationStatus;
        UBGeoLog(@"[urlbox-geo] status=%ld locationServicesEnabled=%d", (long)status,
                 (int)[CLLocationManager locationServicesEnabled]);
        switch (status) {
        case kCLAuthorizationStatusNotDetermined:
            // macOS 与 iOS 不同：requestWhenInUseAuthorization 在 macOS 上不会弹框
            // （实测授权状态永远停在 0）。直接开始定位，系统才会弹授权框。
            gDelegate.started = YES;
            [gDelegate.manager startUpdatingLocation];
            break;
        case kCLAuthorizationStatusAuthorized:
            gDelegate.started = YES;
            [gDelegate.manager requestLocation];
            break;
        case kCLAuthorizationStatusDenied:
            [gDelegate finishWithOk:NO
                                 lat:0
                                 lng:0
                                 acc:0
                                 err:@"定位权限被拒绝（系统设置 → 隐私与安全 → 定位服务）"];
            break;
        default:
            [gDelegate finishWithOk:NO lat:0 lng:0 acc:0 err:@"定位不可用"];
            break;
        }
    }
}

void urlbox_geo_request(UBGeoCallback callback, void *ctx) {
    UBGeoLog(@"[urlbox-geo] request received");
    if ([NSThread isMainThread]) {
        UBGeoPerformRequest(callback, ctx);
    } else {
        dispatch_async(dispatch_get_main_queue(), ^{ UBGeoPerformRequest(callback, ctx); });
    }
}
