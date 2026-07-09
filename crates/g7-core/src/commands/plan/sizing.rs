#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MemorySizingPreset {
    pub(super) key: &'static str,
    pub(super) label: &'static str,
    pub(super) ram: &'static str,
    pub(super) swap: &'static str,
    pub(super) os_reserve: &'static str,
    pub(super) php_max_children: &'static str,
    pub(super) php_processes: &'static str,
    pub(super) php_cpu_guard: &'static str,
    pub(super) php_memory_limit: &'static str,
    pub(super) php_upload_limit: &'static str,
    pub(super) opcache_memory: &'static str,
    pub(super) db_buffer_pool: &'static str,
    pub(super) db_max_connections: &'static str,
    pub(super) db_tmp_table_size: &'static str,
    pub(super) redis_maxmemory: &'static str,
    pub(super) nginx_worker_processes: &'static str,
    pub(super) nginx_worker_connections: &'static str,
    pub(super) nginx_worker_rlimit_nofile: &'static str,
    pub(super) nginx_keepalive_timeout: &'static str,
    pub(super) nginx_fastcgi_buffers: &'static str,
    pub(super) apache_mpm: &'static str,
    pub(super) apache_start_servers: &'static str,
    pub(super) apache_server_limit: &'static str,
    pub(super) apache_threads_per_child: &'static str,
    pub(super) apache_max_request_workers: &'static str,
    pub(super) apache_spare_threads: &'static str,
    pub(super) apache_max_connections_per_child: &'static str,
    pub(super) note: &'static str,
}

pub(super) const MEMORY_SIZING_PRESETS: [MemorySizingPreset; 7] = [
    MemorySizingPreset {
        key: "tier_1gb",
        label: "1GB",
        ram: "0.75-1.5GB",
        swap: "2GB",
        os_reserve: "384M",
        php_max_children: "4",
        php_processes: "start=1,min_spare=1,max_spare=2",
        php_cpu_guard: "min(memory_budget, vCPU*4)",
        php_memory_limit: "192M",
        php_upload_limit: "32M",
        opcache_memory: "64M",
        db_buffer_pool: "128M",
        db_max_connections: "30",
        db_tmp_table_size: "32M",
        redis_maxmemory: "64M",
        nginx_worker_processes: "1",
        nginx_worker_connections: "512",
        nginx_worker_rlimit_nofile: "2048",
        nginx_keepalive_timeout: "15s",
        nginx_fastcgi_buffers: "8 16k",
        apache_mpm: "event",
        apache_start_servers: "1",
        apache_server_limit: "2",
        apache_threads_per_child: "25",
        apache_max_request_workers: "50",
        apache_spare_threads: "min=10,max=25",
        apache_max_connections_per_child: "1000",
        note: "single small site, Redis optional under pressure",
    },
    MemorySizingPreset {
        key: "tier_2gb",
        label: "2GB",
        ram: "1.5-3GB",
        swap: "2GB",
        os_reserve: "512M",
        php_max_children: "8",
        php_processes: "start=2,min_spare=2,max_spare=4",
        php_cpu_guard: "min(memory_budget, vCPU*4)",
        php_memory_limit: "256M",
        php_upload_limit: "64M",
        opcache_memory: "128M",
        db_buffer_pool: "384M",
        db_max_connections: "60",
        db_tmp_table_size: "64M",
        redis_maxmemory: "128M",
        nginx_worker_processes: "min(vCPU,2)",
        nginx_worker_connections: "1024",
        nginx_worker_rlimit_nofile: "4096",
        nginx_keepalive_timeout: "20s",
        nginx_fastcgi_buffers: "16 16k",
        apache_mpm: "event",
        apache_start_servers: "2",
        apache_server_limit: "3",
        apache_threads_per_child: "25",
        apache_max_request_workers: "75",
        apache_spare_threads: "min=25,max=50",
        apache_max_connections_per_child: "2000",
        note: "default low-cost VPS target",
    },
    MemorySizingPreset {
        key: "tier_4gb",
        label: "4GB",
        ram: "3-6GB",
        swap: "2GB",
        os_reserve: "768M",
        php_max_children: "16",
        php_processes: "start=4,min_spare=4,max_spare=8",
        php_cpu_guard: "min(memory_budget, vCPU*6)",
        php_memory_limit: "256M",
        php_upload_limit: "128M",
        opcache_memory: "192M",
        db_buffer_pool: "1G",
        db_max_connections: "100",
        db_tmp_table_size: "128M",
        redis_maxmemory: "256M",
        nginx_worker_processes: "min(vCPU,2)",
        nginx_worker_connections: "2048",
        nginx_worker_rlimit_nofile: "8192",
        nginx_keepalive_timeout: "20s",
        nginx_fastcgi_buffers: "16 16k",
        apache_mpm: "event",
        apache_start_servers: "2",
        apache_server_limit: "4",
        apache_threads_per_child: "25",
        apache_max_request_workers: "100",
        apache_spare_threads: "min=25,max=75",
        apache_max_connections_per_child: "3000",
        note: "small production site baseline",
    },
    MemorySizingPreset {
        key: "tier_8gb",
        label: "8GB",
        ram: "6-12GB",
        swap: "2GB",
        os_reserve: "1G",
        php_max_children: "32",
        php_processes: "start=6,min_spare=6,max_spare=12",
        php_cpu_guard: "min(memory_budget, vCPU*8)",
        php_memory_limit: "256M",
        php_upload_limit: "128M",
        opcache_memory: "256M",
        db_buffer_pool: "2G",
        db_max_connections: "150",
        db_tmp_table_size: "256M",
        redis_maxmemory: "512M",
        nginx_worker_processes: "min(vCPU,4)",
        nginx_worker_connections: "4096",
        nginx_worker_rlimit_nofile: "16384",
        nginx_keepalive_timeout: "30s",
        nginx_fastcgi_buffers: "32 16k",
        apache_mpm: "event",
        apache_start_servers: "3",
        apache_server_limit: "8",
        apache_threads_per_child: "25",
        apache_max_request_workers: "200",
        apache_spare_threads: "min=50,max=100",
        apache_max_connections_per_child: "5000",
        note: "busy single site or light multi-site",
    },
    MemorySizingPreset {
        key: "tier_16gb",
        label: "16GB",
        ram: "12-24GB",
        swap: "4GB",
        os_reserve: "2G",
        php_max_children: "64",
        php_processes: "start=8,min_spare=8,max_spare=16",
        php_cpu_guard: "min(memory_budget, vCPU*10)",
        php_memory_limit: "384M",
        php_upload_limit: "256M",
        opcache_memory: "512M",
        db_buffer_pool: "5G",
        db_max_connections: "250",
        db_tmp_table_size: "512M",
        redis_maxmemory: "1G",
        nginx_worker_processes: "min(vCPU,4)",
        nginx_worker_connections: "8192",
        nginx_worker_rlimit_nofile: "32768",
        nginx_keepalive_timeout: "30s",
        nginx_fastcgi_buffers: "32 32k",
        apache_mpm: "event",
        apache_start_servers: "4",
        apache_server_limit: "12",
        apache_threads_per_child: "25",
        apache_max_request_workers: "300",
        apache_spare_threads: "min=75,max=150",
        apache_max_connections_per_child: "5000",
        note: "high traffic single site",
    },
    MemorySizingPreset {
        key: "tier_32gb",
        label: "32GB",
        ram: "24-32GB",
        swap: "4GB",
        os_reserve: "3G",
        php_max_children: "96",
        php_processes: "start=12,min_spare=12,max_spare=24",
        php_cpu_guard: "min(memory_budget, vCPU*12, 96)",
        php_memory_limit: "512M",
        php_upload_limit: "256M",
        opcache_memory: "768M",
        db_buffer_pool: "10G",
        db_max_connections: "400",
        db_tmp_table_size: "512M",
        redis_maxmemory: "2G",
        nginx_worker_processes: "min(vCPU,8)",
        nginx_worker_connections: "16384",
        nginx_worker_rlimit_nofile: "65535",
        nginx_keepalive_timeout: "30s",
        nginx_fastcgi_buffers: "64 32k",
        apache_mpm: "event",
        apache_start_servers: "4",
        apache_server_limit: "16",
        apache_threads_per_child: "25",
        apache_max_request_workers: "400",
        apache_spare_threads: "min=100,max=200",
        apache_max_connections_per_child: "10000",
        note: "large single site, cap PHP until real traffic is measured",
    },
    MemorySizingPreset {
        key: "tier_gt32gb",
        label: ">32GB",
        ram: "32GB+",
        swap: "4GB fixed unless dump/hibernate policy requires more",
        os_reserve: "max(4GB, RAM*10%)",
        php_max_children: "min(floor(php_budget/128M), 192 per site)",
        php_processes: "start=min(16,max_children/8), spare=25%",
        php_cpu_guard: "min(memory_budget, vCPU*12, 192 per site)",
        php_memory_limit: "512M default, app profile may raise",
        php_upload_limit: "256M default, app profile may raise",
        opcache_memory: "min(max(RAM*2%, 768M), 1G)",
        db_buffer_pool: "min(RAM*40%, 24G) for single DB host",
        db_max_connections: "min(800, RAM_GB*12)",
        db_tmp_table_size: "min(1G, RAM*2%)",
        redis_maxmemory: "min(RAM*6%, 4G)",
        nginx_worker_processes: "min(vCPU,16)",
        nginx_worker_connections: "min(32768, RAM_GB*512)",
        nginx_worker_rlimit_nofile: "min(131072, workers*connections*2)",
        nginx_keepalive_timeout: "30s default, lower under L7 proxy pressure",
        nginx_fastcgi_buffers: "64 32k, tune by response size",
        apache_mpm: "event",
        apache_start_servers: "min(vCPU,8)",
        apache_server_limit: "ceil(max_request_workers/25)",
        apache_threads_per_child: "25",
        apache_max_request_workers: "min(vCPU*64, 800 per site)",
        apache_spare_threads: "25% of max workers",
        apache_max_connections_per_child: "10000",
        note: "formula mode; cap per site before multi-tenant support",
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMemorySizing {
    pub tier_key: String,
    pub tier_label: String,
    pub total_memory_kib: u64,
    pub vcpu_count: usize,
    pub swap_size: String,
    pub php_max_children: u16,
    pub php_start_servers: u16,
    pub php_min_spare_servers: u16,
    pub php_max_spare_servers: u16,
    pub php_memory_limit: String,
    pub php_upload_limit: String,
    pub opcache_memory: String,
    pub db_buffer_pool: String,
    pub db_max_connections: u16,
    pub db_tmp_table_size: String,
    pub redis_maxmemory: String,
    pub nginx_worker_processes: u16,
    pub nginx_worker_connections: u32,
    pub nginx_worker_rlimit_nofile: u32,
    pub nginx_keepalive_timeout: String,
    pub nginx_fastcgi_buffers: String,
    pub apache_max_request_workers: u16,
    pub note: String,
}

pub fn resolve_memory_sizing(total_memory_kib: u64, vcpu_count: usize) -> ResolvedMemorySizing {
    let total_mib = (total_memory_kib / 1024).max(1);
    let vcpu_count = vcpu_count.max(1);
    let preset = memory_preset_for_mib(total_mib);

    if preset.key == "tier_gt32gb" {
        return resolved_formula_sizing(preset, total_memory_kib, total_mib, vcpu_count);
    }

    let (php_start_servers, php_min_spare_servers, php_max_spare_servers) =
        php_process_counts_for_preset(preset.key);
    let nginx_worker_processes = nginx_worker_processes_for_preset(preset.key, vcpu_count);
    let db_max_connections = preset.db_max_connections.parse::<u16>().unwrap_or(100);
    let nginx_worker_connections = preset
        .nginx_worker_connections
        .parse::<u32>()
        .unwrap_or(1024);
    let nginx_worker_rlimit_nofile = preset
        .nginx_worker_rlimit_nofile
        .parse::<u32>()
        .unwrap_or(4096);
    let apache_max_request_workers = preset
        .apache_max_request_workers
        .parse::<u16>()
        .unwrap_or(100);

    ResolvedMemorySizing {
        tier_key: preset.key.to_string(),
        tier_label: preset.label.to_string(),
        total_memory_kib,
        vcpu_count,
        swap_size: canonical_swap_size(preset.swap),
        php_max_children: preset.php_max_children.parse::<u16>().unwrap_or(8),
        php_start_servers,
        php_min_spare_servers,
        php_max_spare_servers,
        php_memory_limit: preset.php_memory_limit.to_string(),
        php_upload_limit: preset.php_upload_limit.to_string(),
        opcache_memory: preset.opcache_memory.to_string(),
        db_buffer_pool: preset.db_buffer_pool.to_string(),
        db_max_connections,
        db_tmp_table_size: preset.db_tmp_table_size.to_string(),
        redis_maxmemory: preset.redis_maxmemory.to_string(),
        nginx_worker_processes,
        nginx_worker_connections,
        nginx_worker_rlimit_nofile,
        nginx_keepalive_timeout: preset.nginx_keepalive_timeout.to_string(),
        nginx_fastcgi_buffers: preset.nginx_fastcgi_buffers.to_string(),
        apache_max_request_workers,
        note: preset.note.to_string(),
    }
}

pub(super) fn memory_preset_for_mib(total_mib: u64) -> &'static MemorySizingPreset {
    if total_mib <= 1536 {
        &MEMORY_SIZING_PRESETS[0]
    } else if total_mib <= 3072 {
        &MEMORY_SIZING_PRESETS[1]
    } else if total_mib <= 6144 {
        &MEMORY_SIZING_PRESETS[2]
    } else if total_mib <= 12288 {
        &MEMORY_SIZING_PRESETS[3]
    } else if total_mib <= 24576 {
        &MEMORY_SIZING_PRESETS[4]
    } else if total_mib <= 32768 {
        &MEMORY_SIZING_PRESETS[5]
    } else {
        &MEMORY_SIZING_PRESETS[6]
    }
}

pub(super) fn php_process_counts_for_preset(key: &str) -> (u16, u16, u16) {
    match key {
        "tier_1gb" => (1, 1, 2),
        "tier_2gb" => (2, 2, 4),
        "tier_4gb" => (4, 4, 8),
        "tier_8gb" => (6, 6, 12),
        "tier_16gb" => (8, 8, 16),
        "tier_32gb" => (12, 12, 24),
        _ => (16, 16, 48),
    }
}

pub(super) fn nginx_worker_processes_for_preset(key: &str, vcpu_count: usize) -> u16 {
    let cap = match key {
        "tier_1gb" => 1,
        "tier_2gb" | "tier_4gb" => 2,
        "tier_8gb" | "tier_16gb" => 4,
        "tier_32gb" => 8,
        _ => 16,
    };
    vcpu_count.min(cap).max(1) as u16
}

pub(super) fn resolved_formula_sizing(
    preset: &'static MemorySizingPreset,
    total_memory_kib: u64,
    total_mib: u64,
    vcpu_count: usize,
) -> ResolvedMemorySizing {
    let ram_gb = (total_mib / 1024).max(33);
    let php_max_children = ((total_mib / 4) / 128)
        .max(32)
        .min((vcpu_count as u64 * 12).min(192)) as u16;
    let php_start_servers = (php_max_children / 8).clamp(4, 16);
    let php_spare = (php_max_children / 4).max(8);
    let opcache_mib = ((total_mib * 2) / 100).clamp(768, 1024);
    let db_buffer_pool_gb = ((ram_gb * 40) / 100).clamp(10, 24);
    let db_max_connections = (ram_gb * 12).clamp(400, 800) as u16;
    let db_tmp_table_mib = ((total_mib * 2) / 100).clamp(512, 1024);
    let redis_mib = ((total_mib * 6) / 100).clamp(2048, 4096);
    let nginx_worker_processes = vcpu_count.clamp(1, 16) as u16;
    let nginx_worker_connections = (ram_gb * 512).clamp(16384, 32768) as u32;
    let nginx_worker_rlimit_nofile =
        (nginx_worker_processes as u32 * nginx_worker_connections * 2).clamp(65535, 131072);
    let apache_max_request_workers = (vcpu_count as u16 * 64).clamp(400, 800);

    ResolvedMemorySizing {
        tier_key: preset.key.to_string(),
        tier_label: preset.label.to_string(),
        total_memory_kib,
        vcpu_count,
        swap_size: canonical_swap_size(preset.swap),
        php_max_children,
        php_start_servers,
        php_min_spare_servers: php_spare,
        php_max_spare_servers: php_spare.saturating_mul(2).min(php_max_children),
        php_memory_limit: "512M".to_string(),
        php_upload_limit: "256M".to_string(),
        opcache_memory: format!("{opcache_mib}M"),
        db_buffer_pool: format!("{db_buffer_pool_gb}G"),
        db_max_connections,
        db_tmp_table_size: format!("{db_tmp_table_mib}M"),
        redis_maxmemory: format!("{redis_mib}M"),
        nginx_worker_processes,
        nginx_worker_connections,
        nginx_worker_rlimit_nofile,
        nginx_keepalive_timeout: "30s".to_string(),
        nginx_fastcgi_buffers: "64 32k".to_string(),
        apache_max_request_workers,
        note: preset.note.to_string(),
    }
}

pub(super) fn canonical_swap_size(value: &str) -> String {
    value.split_whitespace().next().unwrap_or("2GB").to_string()
}
