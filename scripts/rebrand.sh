#!/bin/bash

# Auto Tender Rebranding Script
# This script helps rebrand remaining Zed references to Auto Tender

echo "🔄 Starting Auto Tender rebranding process..."

# Update environment variable references
echo "📝 Updating environment variables..."
find . -name "*.rs" -type f -exec sed -i.bak 's/ZED_/AUTO_TENDER_/g' {} \;

# Update URL schemes and links
echo "🔗 Updating URL schemes..."
find . -name "*.rs" -type f -exec sed -i.bak 's/zed:\/\//autotender:\/\//g' {} \;
find . -name "*.rs" -type f -exec sed -i.bak 's/zed-cli:\/\//autotender-cli:\/\//g' {} \;

# Update documentation references
echo "📚 Updating documentation..."
find . -name "*.md" -type f -exec sed -i.bak 's/zed\.dev/autotender.dev/g' {} \;
find . -name "*.md" -type f -exec sed -i.bak 's/Zed Industries/Auto Tender Team/g' {} \;

# Update desktop files
echo "🖥️  Updating desktop files..."
find . -name "*.desktop*" -type f -exec sed -i.bak 's/zed/auto-tender/g' {} \;

# Update flatpak metadata
echo "📦 Updating flatpak metadata..."
find . -name "*.metainfo.xml*" -type f -exec sed -i.bak 's/Zed/Auto Tender/g' {} \;
find . -name "*.metainfo.xml*" -type f -exec sed -i.bak 's/zed\.dev/autotender.dev/g' {} \;

# Update extension references (be careful with these)
echo "🔌 Updating extension references..."
find extensions/ -name "*.toml" -type f -exec sed -i.bak 's/zed-industries/auto-tender-team/g' {} \;
find extensions/ -name "*.toml" -type f -exec sed -i.bak 's/zed\.dev/autotender.dev/g' {} \;

# Clean up backup files
echo "🧹 Cleaning up backup files..."
find . -name "*.bak" -type f -delete

echo "✅ Rebranding complete!"
echo ""
echo "📋 Manual tasks remaining:"
echo "1. Update app icons in resources/ directory"
echo "2. Update any hardcoded 'Zed' strings in UI text"
echo "3. Update GitHub repository references"
echo "4. Update any remaining extension API references"
echo "5. Test the build to ensure everything works"
echo ""
echo "🚀 Your Auto Tender app is ready!" 